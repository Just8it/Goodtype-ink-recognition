//! Bounded, non-expanding LaTeX math parser for recognizer proposals (ADR 0040).
//! Unsupported constructs are refused; this never executes TeX or Typst code.
//! Arrays accept centered columns only. Aligned is top-level only. Literal text
//! accepts no TeX commands/specials; upright/operator names accept letters only.
//! Empty entries in matrices remain empty content, never a numeric zero.

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Char(char),
    Command(String),
    Literal(String),
}

#[derive(Clone)]
struct Piece {
    text: String,
    atom: bool,
}
impl Piece {
    fn atom(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            atom: true,
        }
    }
    fn compound(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            atom: false,
        }
    }
    fn grouped(&self) -> String {
        if self.atom {
            self.text.clone()
        } else {
            format!("({})", self.text)
        }
    }
}
fn joined(mut parts: Vec<Piece>) -> Piece {
    parts.retain(|p| !p.text.is_empty());
    if parts.len() == 1 {
        parts.remove(0)
    } else {
        Piece::compound(
            parts
                .into_iter()
                .map(|p| p.text)
                .collect::<Vec<_>>()
                .join(" "),
        )
    }
}
/// The bracket a translated piece consists of, alone or carrying scripts like `)^2`.
fn bracket(text: &str) -> Option<char> {
    let first = text.chars().next().filter(|c| "()[]{}".contains(*c))?;
    let rest = &text[1..];
    (rest.is_empty() || rest.starts_with(['_', '^', '\''])).then_some(first)
}
/// A bracket Typst must not pair with another one, escaped so it stays a plain symbol.
fn lone_delimiter(delimiter: String) -> String {
    if matches!(delimiter.as_str(), "(" | ")" | "[" | "]" | "{" | "}") {
        format!("\\{delimiter}")
    } else {
        delimiter
    }
}
fn quoted(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}
fn escaped(c: char, call: bool) -> String {
    if matches!(
        c,
        '#' | '$' | '\\' | '"' | '_' | '^' | '&' | '/' | '@' | ';' | '~'
    ) || (call && c == ',')
    {
        format!("\\{c}")
    } else {
        symbol_name(c).map_or_else(|| c.to_string(), str::to_owned)
    }
}

pub fn latex_to_typst(source: &str) -> Result<String, String> {
    if source.len() > 16 * 1024 {
        return Err("LaTeX input exceeds 16 KiB".into());
    }
    let mut chars = source.chars().peekable();
    let mut tokens = Vec::new();
    while let Some(c) = chars.next() {
        if c == '\\' {
            let first = chars.next().ok_or("unfinished command \\")?;
            let mut name = first.to_string();
            if first.is_ascii_alphabetic() {
                while chars.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
                    name.push(chars.next().unwrap());
                }
                if name == "operatorname" && chars.peek() == Some(&'*') {
                    name.push(chars.next().unwrap());
                }
            }
            let literal = LITERAL_COMMANDS.contains(&name.as_str());
            tokens.push(Token::Command(name));
            if literal {
                while chars.peek().is_some_and(|c| c.is_whitespace()) {
                    chars.next();
                }
                if chars.next() != Some('{') {
                    return Err("expected literal braced argument".into());
                }
                let mut value = String::new();
                let mut closed = false;
                for c in chars.by_ref() {
                    match c {
                        '}' => {
                            closed = true;
                            break;
                        }
                        '{' | '\\' => {
                            return Err("nested or command-bearing literal argument".into());
                        }
                        c if c.is_control() && !c.is_whitespace() => {
                            return Err("control character in text".into());
                        }
                        c if "#$%&^_~".contains(c) => {
                            return Err("TeX special character in literal argument".into());
                        }
                        c => value.push(c),
                    }
                }
                if !closed {
                    return Err("unbalanced literal argument".into());
                }
                tokens.push(Token::Literal(
                    value.split_whitespace().collect::<Vec<_>>().join(" "),
                ));
            }
        } else if !c.is_whitespace() {
            tokens.push(Token::Char(c));
        }
        if tokens.len() > 4096 {
            return Err("LaTeX exceeds 4096 tokens".into());
        }
    }
    let mut parser = Parser { tokens, pos: 0 };
    let result = parser.row(0, false)?.text;
    if parser.pos != parser.tokens.len() {
        return Err(format!("unbalanced or stray {}", parser.label()));
    }
    if result.is_empty() {
        Err("empty formula".into())
    } else {
        Ok(result)
    }
}
struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}
impl Parser {
    fn char(&self, c: char) -> bool {
        self.tokens.get(self.pos) == Some(&Token::Char(c))
    }
    fn cmd(&self, c: &str) -> bool {
        self.tokens.get(self.pos) == Some(&Token::Command(c.into()))
    }
    fn label(&self) -> String {
        format!("{:?}", self.tokens.get(self.pos))
    }
    fn depth(depth: usize) -> Result<(), String> {
        if depth > 64 {
            Err("LaTeX nesting exceeds 64".into())
        } else {
            Ok(())
        }
    }
    fn row(&mut self, depth: usize, call: bool) -> Result<Piece, String> {
        Self::depth(depth)?;
        let mut parts = Vec::new();
        while self.pos < self.tokens.len()
            && !self.char('}')
            && !self.char('&')
            && !self.cmd("\\")
            && !self.cmd("end")
            && !self.cmd("right")
        {
            parts.push(self.piece(depth, call)?);
        }
        // Typst pairs brackets within one row; one left open or closing nothing would swallow
        // the rest of the formula, or the call around it. Those stay plain symbols.
        let mut open: Vec<(usize, char)> = Vec::new();
        let mut lone = Vec::new();
        for (index, part) in parts.iter().enumerate() {
            match bracket(&part.text) {
                Some(c @ ('(' | '[' | '{')) => open.push((index, c)),
                Some(closing) => {
                    let opening = match closing {
                        ')' => '(',
                        ']' => '[',
                        _ => '{',
                    };
                    if open.last().is_some_and(|(_, c)| *c == opening) {
                        open.pop();
                    } else {
                        lone.push(index);
                    }
                }
                None => {}
            }
        }
        for index in lone
            .into_iter()
            .chain(open.into_iter().map(|(index, _)| index))
        {
            parts[index].text.insert(0, '\\');
        }
        Ok(joined(parts))
    }
    fn group(&mut self, depth: usize, call: bool) -> Result<Piece, String> {
        if !self.char('{') {
            return Err("expected braced argument".into());
        }
        self.pos += 1;
        let part = self.row(depth + 1, call)?;
        if !self.char('}') {
            return Err("unbalanced braces".into());
        }
        self.pos += 1;
        Ok(part)
    }
    fn argument(&mut self, depth: usize, call: bool) -> Result<Piece, String> {
        if self.char('{') {
            self.group(depth, call)
        } else {
            self.atom(depth + 1, call, true)
        }
    }
    /// An optional `[...]` argument, parsed from its own bounded token slice so braced groups
    /// inside it stay intact. `None` when there is none; an empty or unbalanced one is refused.
    fn bracket_argument(&mut self, depth: usize, what: &str) -> Result<Option<String>, String> {
        if !self.char('[') {
            return Ok(None);
        }
        self.pos += 1;
        let start = self.pos;
        let mut level = 0;
        while self.pos < self.tokens.len() {
            if self.char(']') && level == 0 {
                break;
            }
            if self.char('[') {
                level += 1;
            } else if self.char(']') {
                level -= 1;
            }
            self.pos += 1;
        }
        if !self.char(']') {
            return Err(format!("unbalanced {what}"));
        }
        let mut inner = Parser {
            tokens: self.tokens[start..self.pos].to_vec(),
            pos: 0,
        };
        let value = inner.row(depth + 1, true)?;
        if inner.pos != inner.tokens.len() || value.text.is_empty() {
            return Err(format!("invalid {what}"));
        }
        self.pos += 1;
        Ok(Some(value.text))
    }

    fn piece(&mut self, depth: usize, call: bool) -> Result<Piece, String> {
        let mut base = self.atom(depth, call, false)?;
        // `\sum\limits_{i}`: where the scripts go, decided on the operator itself.
        if self.cmd("limits") || self.cmd("nolimits") {
            if base.text.is_empty() {
                return Err("\\limits without an operator".into());
            }
            let function = if self.cmd("limits") {
                "limits"
            } else {
                "scripts"
            };
            self.pos += 1;
            base = Piece::atom(format!("{function}({})", base.text));
        }
        let mut sub = None;
        let mut sup = None;
        let mut primes = String::new();
        loop {
            if self.char('_') || self.char('^') {
                if base.text.is_empty() {
                    return Err("script without a base".into());
                }
                let is_sub = self.char('_');
                self.pos += 1;
                let arg = self.argument(depth, call)?;
                if arg.text.is_empty() {
                    return Err("empty script".into());
                }
                let slot = if is_sub { &mut sub } else { &mut sup };
                if slot.replace(arg).is_some() {
                    return Err("duplicate script".into());
                }
            } else if self.char('\'') || self.cmd("prime") {
                if base.text.is_empty() {
                    return Err("prime without a base".into());
                }
                self.pos += 1;
                primes.push('\'');
            } else {
                break;
            }
        }
        if !primes.is_empty() && sup.is_some() {
            return Err("prime combined with superscript".into());
        }
        if sub.is_some() || sup.is_some() || !primes.is_empty() {
            // `(^{2}`: the script belongs to the bracket itself, which Typst would open as a group.
            if matches!(base.text.as_str(), "(" | "[" | "{") {
                base.text = lone_delimiter(base.text);
            }
            let mut text = base.grouped();
            text.push_str(&primes);
            if let Some(sub) = sub {
                text.push_str(&format!("_{}", sub.grouped()));
            }
            if let Some(sup) = sup {
                if !sup.text.is_empty() && sup.text.chars().all(|c| c == '\'') {
                    text.push_str(&sup.text);
                } else {
                    text.push_str(&format!("^{}", sup.grouped()));
                }
            }
            base = Piece::compound(text);
        }
        Ok(base)
    }
    fn raw_group(&mut self) -> Result<String, String> {
        if let Some(Token::Literal(value)) = self.tokens.get(self.pos) {
            self.pos += 1;
            return Ok(value.clone());
        }
        if !self.char('{') {
            return Err("expected literal braced argument".into());
        }
        self.pos += 1;
        let mut text = String::new();
        while let Some(token) = self.tokens.get(self.pos) {
            match token {
                Token::Char('}') => {
                    self.pos += 1;
                    return Ok(text);
                }
                Token::Char('{') | Token::Command(_) | Token::Literal(_) => {
                    return Err("nested or command-bearing literal argument".into());
                }
                Token::Char(c) => text.push(*c),
            }
            self.pos += 1;
        }
        Err("unbalanced literal argument".into())
    }
    fn atom(&mut self, depth: usize, call: bool, single: bool) -> Result<Piece, String> {
        Self::depth(depth)?;
        if self.char('{') {
            return self.group(depth, call);
        }
        let token = self
            .tokens
            .get(self.pos)
            .cloned()
            .ok_or("missing argument")?;
        self.pos += 1;
        match token {
            Token::Char(c) if c.is_ascii_digit() => {
                let mut number = c.to_string();
                if !single {
                    while let Some(Token::Char(n)) = self.tokens.get(self.pos) {
                        if n.is_ascii_digit()
                            || (*n == '.'
                                && !number.contains('.')
                                && matches!(self.tokens.get(self.pos+1), Some(Token::Char(c)) if c.is_ascii_digit()))
                        {
                            number.push(*n);
                            self.pos += 1;
                        } else {
                            break;
                        }
                    }
                }
                Ok(Piece::atom(number))
            }
            Token::Char(c)
                if c.is_alphabetic()
                    || "+-=<>!()[]|.,:;*/?".contains(c)
                    || symbol_name(c).is_some() =>
            {
                Ok(Piece::atom(escaped(c, call)))
            }
            Token::Char('~') => Ok(Piece::atom("space")),
            Token::Char(c) => Err(format!("unsupported or stray character {c}")),
            Token::Command(name) => self.command(&name, depth, call),
            Token::Literal(_) => Err("stray literal argument".into()),
        }
    }
    fn command(&mut self, name: &str, depth: usize, call: bool) -> Result<Piece, String> {
        if let Some(symbol) = table(COMMAND_SYMBOLS, name) {
            return Ok(Piece::atom(symbol));
        }
        if TYPST_OPERATORS.contains(&name) {
            return Ok(Piece::atom(name));
        }
        match name {
            "," => Ok(Piece::atom("thin")),
            ":" | ";" => Ok(Piece::atom(if name == ":" { "med" } else { "thick" })),
            "!" | "negthinspace" => Ok(Piece::atom("#h(-1em / 6)")),
            " " => Ok(Piece::atom("space")),
            // The block is already display math.
            "displaystyle" => Ok(Piece::atom("")),
            "{" | "}" | "|" | "#" | "$" | "%" | "&" | "_" => Ok(Piece::atom(if name == "|" {
                "‖".into()
            } else {
                escaped(name.chars().next().unwrap(), call)
            })),
            // A manual size on a lone delimiter; Typst sizes delimiters itself.
            "big" | "Big" | "bigg" | "Bigg" | "bigl" | "Bigl" | "biggl" | "Biggl" | "bigr"
            | "Bigr" | "biggr" | "Biggr" | "bigm" | "Bigm" | "biggm" | "Biggm" => {
                Ok(Piece::atom(lone_delimiter(self.delimiter()?)))
            }
            "not" => {
                let negated = match self.tokens.get(self.pos) {
                    Some(Token::Char(c)) => table(NEGATED_CHARS, &c.to_string()),
                    Some(Token::Command(n)) => table(NEGATED_COMMANDS, n),
                    _ => None,
                }
                .ok_or("unsupported \\not")?;
                self.pos += 1;
                Ok(Piece::atom(negated))
            }
            "frac" | "dfrac" | "tfrac" | "cfrac" => {
                let a = self.group(depth, call)?;
                let b = self.group(depth, call)?;
                if a.text.is_empty() || b.text.is_empty() {
                    return Err("empty fraction part".into());
                }
                Ok(Piece::compound(format!("({})/({})", a.text, b.text)))
            }
            "binom" | "dbinom" | "tbinom" => {
                let n = self.group(depth, true)?;
                let k = self.group(depth, true)?;
                if n.text.is_empty() || k.text.is_empty() {
                    return Err("empty binomial part".into());
                }
                Ok(Piece::atom(format!("binom({}, {})", n.text, k.text)))
            }
            "pmod" => {
                let body = self.group(depth, call)?;
                if body.text.is_empty() {
                    return Err("empty \\pmod".into());
                }
                Ok(Piece::atom(format!("(mod {})", body.text)))
            }
            "boxed" => {
                let body = self.group(depth, false)?;
                if body.text.is_empty() {
                    return Err("empty \\boxed".into());
                }
                Ok(Piece::atom(format!(
                    "#box(stroke: 0.5pt, inset: 3pt, ${}$)",
                    body.text
                )))
            }
            // `\sum_{\substack{i<n\\j>0}}`: stacked conditions under one operator.
            "substack" => {
                if !self.char('{') {
                    return Err("expected braced argument".into());
                }
                self.pos += 1;
                let mut lines = vec![self.row(depth + 1, call)?.text];
                while self.cmd("\\") {
                    self.pos += 1;
                    lines.push(self.row(depth + 1, call)?.text);
                }
                if !self.char('}') {
                    return Err("unbalanced \\substack".into());
                }
                self.pos += 1;
                if lines.iter().all(String::is_empty) {
                    return Err("empty \\substack".into());
                }
                Ok(Piece::compound(lines.join(" \\ ")))
            }
            // The annotation follows as an ordinary script and goes under or over the brace.
            "overbrace" | "underbrace" | "overbracket" | "underbracket" => {
                let body = self.group(depth, true)?;
                if body.text.is_empty() {
                    return Err(format!("empty \\{name} argument"));
                }
                let script = if name.starts_with("over") { '^' } else { '_' };
                let annotation = if self.char(script) {
                    self.pos += 1;
                    let note = self.argument(depth, true)?;
                    if note.text.is_empty() {
                        return Err("empty script".into());
                    }
                    format!(", {}", note.text)
                } else {
                    String::new()
                };
                Ok(Piece::atom(format!("{name}({}{annotation})", body.text)))
            }
            "sqrt" => {
                let index = self.bracket_argument(depth, "root index")?;
                let body = self.group(depth, true)?.text;
                if body.is_empty() {
                    return Err("empty square root".into());
                }
                Ok(Piece::atom(if let Some(n) = index {
                    format!("root({n}, {body})")
                } else {
                    format!("sqrt({body})")
                }))
            }
            name if table(STYLE_FUNCTIONS, name).is_some() => {
                let function = table(STYLE_FUNCTIONS, name).unwrap();
                let body = self.group(depth, true)?;
                if body.text.is_empty() {
                    return Err(format!("empty \\{name} argument"));
                }
                Ok(Piece::atom(format!("{function}({})", body.text)))
            }
            "operatorname" | "operatorname*" | "mathrm" => {
                // Math mode ignores spaces, and the recognizer spaces letters: `\mathrm { d x }`.
                let raw: String = self.raw_group()?.split_whitespace().collect();
                if raw.is_empty() {
                    return Err(format!("empty \\{name}"));
                }
                let allowed =
                    |c: char| c.is_alphabetic() || (name == "mathrm" && c.is_ascii_digit());
                if !raw.chars().all(allowed) {
                    return Err(format!("unsupported \\{name} argument"));
                }
                Ok(Piece::atom(if TYPST_OPERATORS.contains(&raw.as_str()) {
                    raw
                } else if name == "operatorname*" {
                    format!("op({}, limits: #true)", quoted(&raw))
                } else if name == "operatorname" {
                    format!("op({})", quoted(&raw))
                } else {
                    quoted(&raw)
                }))
            }
            "text" | "textrm" | "textnormal" | "mbox" | "textbf" | "textit" => {
                let raw = self.raw_group()?;
                if raw.is_empty() {
                    return Err(format!("empty \\{name}"));
                }
                Ok(Piece::atom(match name {
                    "textbf" => format!("bold({})", quoted(&raw)),
                    "textit" => format!("italic({})", quoted(&raw)),
                    _ => quoted(&raw),
                }))
            }
            // Extensible relations, `\xrightarrow[below]{above}`: a stretched symbol with the text
            // attached over and under it, which is how Typst spells the same thing.
            "xlongequal" | "xrightarrow" | "xleftarrow" | "xRightarrow" | "xLeftarrow"
            | "xleftrightarrow" | "xLeftrightarrow" | "xmapsto" => {
                let symbol = match name {
                    "xlongequal" => "=",
                    "xrightarrow" => "arrow.r",
                    "xleftarrow" => "arrow.l",
                    "xRightarrow" => "arrow.r.double",
                    "xLeftarrow" => "arrow.l.double",
                    "xleftrightarrow" => "arrow.l.r",
                    "xLeftrightarrow" => "arrow.l.r.double",
                    _ => "arrow.r.bar",
                };
                let below = self.bracket_argument(depth, "extensible arrow argument")?;
                let above = self.group(depth, call)?;
                let mut text = format!("stretch({symbol})");
                if let Some(below) = below {
                    text.push_str(&format!("_({below})"));
                }
                if !above.text.is_empty() {
                    text.push_str(&format!("^{}", above.grouped()));
                }
                Ok(Piece::compound(text))
            }
            "overset" | "underset" | "stackrel" => {
                let script = self.group(depth, call)?;
                let base = self.group(depth, true)?;
                if base.text.is_empty() || script.text.is_empty() {
                    return Err(format!("empty \\{name} argument"));
                }
                Ok(Piece::compound(format!(
                    "limits({}){}{}",
                    base.text,
                    if name == "underset" { '_' } else { '^' },
                    script.grouped()
                )))
            }
            "left" => {
                let open = self.delimiter()?;
                let body = self.row(depth + 1, true)?;
                if !self.cmd("right") {
                    return Err("unbalanced \\left/\\right".into());
                }
                self.pos += 1;
                let close = self.delimiter()?;
                if open.is_empty() && close.is_empty() && body.text.is_empty() {
                    return Err("empty \\left/\\right".into());
                }
                // `\left\{ ... \right.` would leave Typst an unclosed bracket inside the call.
                let paired = matches!(
                    (open.as_str(), close.as_str()),
                    ("(", ")") | ("[", "]") | ("{", "}")
                );
                let (open, close) = if paired {
                    (open, close)
                } else {
                    (lone_delimiter(open), lone_delimiter(close))
                };
                Ok(Piece::atom(format!("lr({open} {} {close})", body.text)))
            }
            "prime" => Ok(Piece::atom("'")),
            "begin" => self.environment(depth),
            _ => Err(format!("unsupported command \\{name}")),
        }
    }
    fn delimiter(&mut self) -> Result<String, String> {
        let token = self
            .tokens
            .get(self.pos)
            .cloned()
            .ok_or("missing delimiter")?;
        self.pos += 1;
        match token {
            Token::Char('.') => Ok(String::new()),
            Token::Char(c) if "()[]|/".contains(c) => Ok(escaped(c, true)),
            Token::Char('<') => Ok("⟨".into()),
            Token::Char('>') => Ok("⟩".into()),
            Token::Command(n) => match n.as_str() {
                "{" | "lbrace" => Ok("{".into()),
                "}" | "rbrace" => Ok("}".into()),
                "lbrack" => Ok("[".into()),
                "rbrack" => Ok("]".into()),
                "langle" => Ok("⟨".into()),
                "rangle" => Ok("⟩".into()),
                "lvert" | "rvert" | "vert" => Ok("|".into()),
                "|" | "Vert" | "lVert" | "rVert" => Ok("‖".into()),
                "lfloor" => Ok("⌊".into()),
                "rfloor" => Ok("⌋".into()),
                "lceil" => Ok("⌈".into()),
                "rceil" => Ok("⌉".into()),
                _ => Err(format!("unsupported delimiter \\{n}")),
            },
            _ => Err("unsupported delimiter".into()),
        }
    }
    fn environment(&mut self, depth: usize) -> Result<Piece, String> {
        Self::depth(depth + 1)?;
        let starts_formula = self.pos == 1;
        let name = self.raw_group()?;
        if !matches!(
            name.as_str(),
            "matrix"
                | "pmatrix"
                | "bmatrix"
                | "Bmatrix"
                | "vmatrix"
                | "Vmatrix"
                | "smallmatrix"
                | "array"
                | "cases"
                | "dcases"
                | "rcases"
                | "aligned"
                | "align"
                | "align*"
                | "gathered"
                | "gather"
                | "gather*"
                | "split"
        ) {
            return Err(format!("unsupported environment {name}"));
        }
        // An array of `rl` column pairs is how equation systems are often written; it lines up
        // exactly like `aligned`. Otherwise every column must share one alignment.
        let mut aligned = matches!(
            name.as_str(),
            "aligned" | "align" | "align*" | "gathered" | "gather" | "gather*" | "split"
        );
        let mut column_align = None;
        let columns = if name == "array" {
            let spec = self.raw_group()?;
            let uniform = |c: char| !spec.is_empty() && spec.chars().all(|s| s == c);
            let pairs = spec.len() % 2 == 0 && spec.as_bytes().chunks(2).all(|p| p == b"rl");
            if uniform('l') {
                column_align = Some("left");
            } else if uniform('r') {
                column_align = Some("right");
            } else if pairs && depth == 0 {
                aligned = true;
            } else if !uniform('c') {
                return Err("array column spec (columns must share one alignment)".into());
            }
            Some(spec.len())
        } else {
            None
        };
        if aligned && depth != 0 {
            return Err(format!("nested {name} environment"));
        }
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut cells = Vec::new();
        loop {
            cells.push(self.row(depth + 1, !aligned)?.text);
            if self.char('&') {
                self.pos += 1;
                continue;
            }
            if self.cmd("\\") {
                self.pos += 1;
                rows.push(std::mem::take(&mut cells));
                if self.cmd("end") {
                    break;
                }
                continue;
            }
            if self.cmd("end") {
                break;
            }
            return Err(format!("unbalanced environment {name}"));
        }
        if !cells.is_empty() {
            rows.push(cells);
        }
        self.pos += 1;
        if self.raw_group()? != name {
            return Err(format!("mismatched environment {name}"));
        }
        let width = rows.first().map_or(0, Vec::len);
        if width == 0
            || rows
                .iter()
                .any(|r| r.len() != width || r.iter().all(String::is_empty))
            || columns.is_some_and(|n| n != width)
        {
            return Err(format!("ragged {name} rows"));
        }
        let text = if aligned {
            if !starts_formula || self.pos != self.tokens.len() {
                return Err(format!("{name} must be the whole formula"));
            }
            rows.into_iter()
                .map(|r| r.join(" & "))
                .collect::<Vec<_>>()
                .join(" \\ ")
        } else if name.ends_with("cases") {
            if width > 2 {
                return Err("cases with more than two columns".into());
            }
            format!(
                "cases({}{})",
                if name == "rcases" {
                    "reverse: #true, "
                } else {
                    ""
                },
                rows.into_iter()
                    .map(|r| r.join(" quad "))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else {
            let delim = match name.as_str() {
                "pmatrix" => "\"(\"",
                "bmatrix" => "\"[\"",
                "Bmatrix" => "\"{\"",
                "vmatrix" => "\"|\"",
                "Vmatrix" => "\"||\"",
                _ => "#none",
            };
            let align = column_align.map_or(String::new(), |a| format!("align: #{a}, "));
            format!(
                "mat(delim: {delim}, {align}{})",
                rows.into_iter()
                    .map(|r| r
                        .into_iter()
                        .map(|s| if s.is_empty() { "\"\"".into() } else { s })
                        .collect::<Vec<_>>()
                        .join(", "))
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        };
        Ok(Piece::atom(text))
    }
}

const TYPST_OPERATORS: &[&str] = &[
    "arccos", "arcsin", "arctan", "arg", "cos", "cosh", "cot", "coth", "csc", "csch", "deg", "det",
    "dim", "exp", "gcd", "lcm", "hom", "inf", "ker", "lg", "lim", "liminf", "limsup", "ln", "log",
    "max", "min", "mod", "Pr", "sec", "sech", "sin", "sinh", "sup", "tan", "tanh",
];

/// Commands whose braced argument is read verbatim, never parsed as math.
#[rustfmt::skip]
const LITERAL_COMMANDS: &[&str] = &[
    "text", "textrm", "textnormal", "mbox", "textbf", "textit", "mathrm", "operatorname",
    "operatorname*",
];

fn table(entries: &'static [(&'static str, &'static str)], key: &str) -> Option<&'static str> {
    entries
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, typst)| *typst)
}

/// Symbols with a Typst name the writer can type. Anything else stays as its Unicode character,
/// which Typst also accepts.
fn symbol_name(symbol: char) -> Option<&'static str> {
    CHAR_SYMBOLS
        .iter()
        .find(|(c, _)| *c == symbol)
        .map(|(_, typst)| *typst)
}

#[rustfmt::skip]
const CHAR_SYMBOLS: &[(char, &str)] = &[
    ('α', "alpha"), ('β', "beta"), ('γ', "gamma"), ('δ', "delta"), ('ε', "epsilon"),
    ('ϵ', "epsilon.alt"), ('ζ', "zeta"), ('η', "eta"), ('θ', "theta"), ('ϑ', "theta.alt"),
    ('ι', "iota"), ('κ', "kappa"), ('λ', "lambda"), ('μ', "mu"), ('ν', "nu"), ('ξ', "xi"),
    ('π', "pi"), ('ρ', "rho"), ('σ', "sigma"), ('ς', "sigma.alt"), ('τ', "tau"),
    ('υ', "upsilon"), ('φ', "phi"), ('ϕ', "phi.alt"), ('χ', "chi"), ('ψ', "psi"),
    ('ω', "omega"), ('Γ', "Gamma"), ('Δ', "Delta"), ('Θ', "Theta"), ('Λ', "Lambda"),
    ('Ξ', "Xi"), ('Π', "Pi"), ('Σ', "Sigma"), ('Φ', "Phi"), ('Ψ', "Psi"), ('Ω', "Omega"),
    ('∇', "nabla"), ('∂', "partial"), ('∞', "infinity"), ('∅', "emptyset"), ('∑', "sum"),
    ('∏', "product"), ('∫', "integral"), ('∮', "integral.cont"), ('∃', "exists"),
    ('∀', "forall"), ('∈', "in"), ('∉', "in.not"), ('≠', "eq.not"), ('≤', "lt.eq"),
    ('≥', "gt.eq"), ('≈', "approx"), ('≡', "equiv"), ('±', "plus.minus"), ('∓', "minus.plus"),
    ('×', "times"), ('·', "dot.op"), ('⋅', "dot.op"), ('÷', "div"), ('→', "arrow.r"),
    ('←', "arrow.l"), ('↔', "arrow.l.r"), ('⇒', "arrow.r.double"), ('⇐', "arrow.l.double"),
    ('⇔', "arrow.l.r.double"), ('∪', "union"), ('∩', "inter"), ('⊂', "subset"),
    ('⊆', "subset.eq"), ('∧', "and"), ('∨', "or"), ('¬', "not"), ('∠', "angle"),
    ('°', "degree"), ('−', "-"), ('′', "'"), ('″', "''"),
];

/// LaTeX symbol commands (TeX core, amsmath, amssymb) and their Typst names. The tests compile
/// every entry and refuse deprecated names.
#[rustfmt::skip]
const COMMAND_SYMBOLS: &[(&str, &str)] = &[
    // Greek.
    ("alpha", "alpha"), ("beta", "beta"), ("gamma", "gamma"), ("delta", "delta"),
    ("epsilon", "epsilon.alt"), ("varepsilon", "epsilon"), ("zeta", "zeta"), ("eta", "eta"),
    ("theta", "theta"), ("vartheta", "theta.alt"), ("iota", "iota"), ("kappa", "kappa"),
    ("varkappa", "kappa.alt"), ("lambda", "lambda"), ("mu", "mu"), ("nu", "nu"), ("xi", "xi"),
    ("pi", "pi"), ("varpi", "pi.alt"), ("rho", "rho"), ("varrho", "rho.alt"),
    ("sigma", "sigma"), ("varsigma", "sigma.alt"), ("tau", "tau"), ("upsilon", "upsilon"),
    ("phi", "phi.alt"), ("varphi", "phi"), ("chi", "chi"), ("psi", "psi"), ("omega", "omega"),
    ("Gamma", "Gamma"), ("Delta", "Delta"), ("Theta", "Theta"), ("Lambda", "Lambda"),
    ("Xi", "Xi"), ("Pi", "Pi"), ("Sigma", "Sigma"), ("Upsilon", "Upsilon"), ("Phi", "Phi"),
    ("Psi", "Psi"), ("Omega", "Omega"),
    // Letter-like and miscellaneous.
    ("partial", "partial"), ("nabla", "nabla"), ("infty", "infinity"), ("emptyset", "emptyset"),
    ("varnothing", "emptyset"), ("hbar", "planck"), ("hslash", "planck"), ("ell", "ell"),
    ("wp", "℘"), ("aleph", "aleph"), ("Re", "Re"), ("Im", "Im"), ("imath", "dotless.i"),
    ("jmath", "dotless.j"), ("angle", "angle"), ("measuredangle", "angle.arc"),
    ("sphericalangle", "angle.spheric"), ("triangle", "triangle.stroked.t"),
    ("Box", "square.stroked"), ("square", "square.stroked"), ("diamond", "diamond.stroked.small"),
    ("lozenge", "lozenge.stroked"), ("dagger", "dagger"), ("ddagger", "dagger.double"),
    ("top", "top"), ("bot", "bot"), ("perp", "perp"), ("parallel", "parallel"),
    ("nparallel", "parallel.not"), ("mid", "divides"), ("nmid", "divides.not"),
    ("therefore", "therefore"), ("because", "because"), ("backslash", "backslash"),
    ("setminus", "without"), ("smallsetminus", "without"), ("complement", "complement"),
    // Large operators.
    ("sum", "sum"), ("prod", "product"), ("coprod", "product.co"), ("int", "integral"),
    ("iint", "integral.double"), ("iiint", "integral.triple"), ("oint", "integral.cont"),
    ("oiint", "integral.surf"), ("oiiint", "integral.vol"), ("bigcup", "union.big"),
    ("bigcap", "inter.big"), ("bigsqcup", "union.sq.big"), ("biguplus", "union.plus.big"),
    ("bigvee", "or.big"), ("bigwedge", "and.big"), ("bigoplus", "plus.o.big"),
    ("bigotimes", "times.o.big"), ("bigodot", "dot.o.big"),
    // Binary operators.
    ("pm", "plus.minus"), ("mp", "minus.plus"), ("times", "times"), ("div", "div"),
    ("cdot", "dot.op"), ("cdotp", "dot.c"), ("ast", "ast.op"), ("star", "star.op"),
    ("circ", "compose"), ("bullet", "bullet"), ("oplus", "plus.o"), ("ominus", "minus.o"),
    ("otimes", "times.o"), ("odot", "dot.o"), ("oslash", "slash.o"), ("cup", "union"),
    ("cap", "inter"), ("sqcup", "union.sq"), ("sqcap", "inter.sq"), ("uplus", "union.plus"),
    ("wedge", "and"), ("land", "and"), ("vee", "or"), ("lor", "or"), ("neg", "not"),
    ("lnot", "not"), ("wr", "wreath"), ("bmod", "mod"),
    // Relations.
    ("neq", "eq.not"), ("ne", "eq.not"), ("leq", "lt.eq"), ("le", "lt.eq"), ("geq", "gt.eq"),
    ("ge", "gt.eq"), ("leqslant", "lt.eq.slant"), ("geqslant", "gt.eq.slant"),
    ("leqq", "lt.equiv"), ("geqq", "gt.equiv"), ("nleq", "lt.eq.not"), ("ngeq", "gt.eq.not"),
    ("nless", "lt.not"), ("ngtr", "gt.not"), ("lneq", "lt.neq"), ("gneq", "gt.neq"),
    ("ll", "lt.double"), ("gg", "gt.double"), ("lesssim", "lt.tilde"), ("gtrsim", "gt.tilde"),
    ("lessapprox", "lt.approx"), ("gtrapprox", "gt.approx"), ("prec", "prec"), ("succ", "succ"),
    ("preceq", "prec.eq"), ("succeq", "succ.eq"), ("approx", "approx"),
    ("approxeq", "approx.eq"), ("sim", "tilde.op"), ("nsim", "tilde.not"),
    ("simeq", "tilde.eq"), ("cong", "tilde.equiv"), ("equiv", "equiv"),
    ("nequiv", "equiv.not"), ("asymp", "asymp"), ("doteq", "eq.dot"), ("triangleq", "eq.delta"),
    ("coloneqq", "colon.eq"), ("eqqcolon", "eq.colon"), ("propto", "prop"),
    ("varpropto", "prop"), ("fallingdotseq", "≒"), ("in", "in"), ("notin", "in.not"),
    ("ni", "in.rev"), ("owns", "in.rev"), ("subset", "subset"), ("supset", "supset"),
    ("subseteq", "subset.eq"), ("supseteq", "supset.eq"), ("subsetneq", "subset.neq"),
    ("supsetneq", "supset.neq"), ("nsubseteq", "subset.eq.not"),
    ("nsupseteq", "supset.eq.not"), ("sqsubset", "subset.sq"), ("sqsupset", "supset.sq"),
    ("models", "models"), ("vdash", "tack.r"), ("dashv", "tack.l"), ("smile", "smile"),
    ("frown", "frown"), ("exists", "exists"), ("nexists", "exists.not"), ("forall", "forall"),
    // Arrows.
    ("to", "arrow.r"), ("rightarrow", "arrow.r"), ("gets", "arrow.l"),
    ("leftarrow", "arrow.l"), ("leftrightarrow", "arrow.l.r"),
    ("Rightarrow", "arrow.r.double"), ("Leftarrow", "arrow.l.double"),
    ("Leftrightarrow", "arrow.l.r.double"), ("longrightarrow", "arrow.r.long"),
    ("longleftarrow", "arrow.l.long"), ("longleftrightarrow", "arrow.l.r.long"),
    ("Longrightarrow", "arrow.r.double.long"), ("Longleftarrow", "arrow.l.double.long"),
    ("Longleftrightarrow", "arrow.l.r.double.long"), ("implies", "arrow.r.double.long"),
    ("impliedby", "arrow.l.double.long"), ("iff", "arrow.l.r.double.long"),
    ("mapsto", "arrow.r.bar"), ("longmapsto", "arrow.r.long.bar"),
    ("hookrightarrow", "arrow.r.hook"), ("leadsto", "arrow.r.squiggly"),
    ("uparrow", "arrow.t"), ("downarrow", "arrow.b"), ("updownarrow", "arrow.t.b"),
    ("Uparrow", "arrow.t.double"), ("Downarrow", "arrow.b.double"),
    ("Updownarrow", "arrow.t.b.double"), ("nearrow", "arrow.tr"), ("searrow", "arrow.br"),
    ("nwarrow", "arrow.tl"), ("swarrow", "arrow.bl"), ("rightharpoonup", "harpoon.rt"),
    ("leftharpoonup", "harpoon.lt"), ("rightleftharpoons", "harpoons.rtlb"),
    ("leftrightharpoons", "harpoons.ltrb"),
    // Dots.
    ("ldots", "dots.h"), ("dots", "dots.h"), ("dotsc", "dots.h"), ("hdots", "dots.h"),
    ("cdots", "dots.h.c"), ("dotsb", "dots.h.c"), ("dotsm", "dots.h.c"),
    ("dotsi", "dots.h.c"), ("vdots", "dots.v"), ("ddots", "dots.down"),
    // Delimiters written without \left or \right.
    ("langle", "chevron.l"), ("rangle", "chevron.r"), ("lbrace", "brace.l"),
    ("rbrace", "brace.r"), ("lbrack", "bracket.l"), ("rbrack", "bracket.r"),
    ("lfloor", "floor.l"), ("rfloor", "floor.r"), ("lceil", "ceil.l"), ("rceil", "ceil.r"),
    ("vert", "bar.v"), ("lvert", "bar.v"), ("rvert", "bar.v"), ("Vert", "bar.v.double"),
    ("lVert", "bar.v.double"), ("rVert", "bar.v.double"),
    // Spacing.
    ("quad", "quad"), ("qquad", "wide"), ("thinspace", "thin"), ("medspace", "med"),
    ("thickspace", "thick"), ("enspace", "space.en"),
];

/// `\not` before a relation.
const NEGATED_CHARS: &[(&str, &str)] = &[("=", "eq.not"), ("<", "lt.not"), (">", "gt.not")];
#[rustfmt::skip]
const NEGATED_COMMANDS: &[(&str, &str)] = &[
    ("in", "in.not"), ("ni", "in.rev.not"), ("equiv", "equiv.not"), ("sim", "tilde.not"),
    ("simeq", "tilde.eq.not"), ("approx", "approx.not"), ("subset", "subset.not"),
    ("supset", "supset.not"), ("subseteq", "subset.eq.not"), ("supseteq", "supset.eq.not"),
    ("leq", "lt.eq.not"), ("le", "lt.eq.not"), ("geq", "gt.eq.not"), ("ge", "gt.eq.not"),
    ("parallel", "parallel.not"), ("mid", "divides.not"), ("exists", "exists.not"),
];

/// Accents and alphabets: one braced argument wrapped in the Typst function of the same effect.
#[rustfmt::skip]
const STYLE_FUNCTIONS: &[(&str, &str)] = &[
    ("underline", "underline"), ("overline", "overline"), ("bar", "overline"),
    ("vec", "arrow"), ("overrightarrow", "arrow"), ("overleftarrow", "arrow.l"),
    ("overleftrightarrow", "arrow.l.r"), ("hat", "hat"), ("widehat", "hat"),
    ("tilde", "tilde"), ("widetilde", "tilde"), ("dot", "dot"), ("ddot", "dot.double"),
    ("dddot", "dot.triple"), ("check", "caron"), ("breve", "breve"), ("acute", "acute"),
    ("grave", "grave"), ("mathring", "circle"), ("mathbf", "bold"), ("boldsymbol", "bold"),
    ("bm", "bold"), ("pmb", "bold"), ("mathbb", "bb"), ("mathcal", "cal"), ("mathscr", "scr"),
    ("mathfrak", "frak"), ("mathsf", "sans"), ("mathtt", "mono"), ("mathit", "italic"),
];

#[cfg(test)]
mod tests {
    use super::*;
    const SAMPLES: &[(&str, &str)] = &[
        ("ab+1.5", "a b + 1.5"),
        (r"x_1^2", "x_1^2"),
        (r"x^{ab}_{i+1}", "x_(i + 1)^(a b)"),
        (r"\frac{a+b}{c}", "(a + b)/(c)"),
        (r"\sqrt{x,y}", r"sqrt(x \, y)"),
        (r"\sqrt[3]{x}", "root(3, x)"),
        (r"a\xlongequal{\text{def}}b", r#"a stretch(=)^"def" b"#),
        (
            r"x\xrightarrow[n]{t\to 0}y",
            "x stretch(arrow.r)_(n)^(t arrow.r 0) y",
        ),
        (r"A\xRightarrow{}B", "A stretch(arrow.r.double) B"),
        (r"\left( x+1 \right)", "lr(( x + 1 ))"),
        (r"\left. x \right|_0", "lr( x |)_0"),
        (
            r"\alpha\epsilon\varepsilon\phi\varphi\Gamma",
            "alpha epsilon.alt epsilon phi.alt phi Gamma",
        ),
        (r"\partial\nabla\infty", "partial nabla infinity"),
        (
            r"\neq\leq\le\geq\ge\approx\equiv\sim\fallingdotseq",
            "eq.not lt.eq lt.eq gt.eq gt.eq approx equiv tilde.op ≒",
        ),
        (
            r"\Rightarrow\to\rightarrow\Leftrightarrow",
            "arrow.r.double arrow.r arrow.r arrow.l.r.double",
        ),
        (r"\cdot\times\pm", "dot.op times plus.minus"),
        (
            r"\sum_{j=0}^{m}x_j+\prod_i y_i+\int_0^1 z",
            "sum_(j = 0)^m x_j + product_i y_i + integral_0^1 z",
        ),
        (
            r"\lim_{x\to0}\sin x+\cos x+\ln x+\log x+\exp x",
            "lim_(x arrow.r 0) sin x + cos x + ln x + log x + exp x",
        ),
        (
            r"\operatorname{x}\operatorname{sin}\mathrm{const}\mathrm{cos}",
            "op(\"x\") sin \"const\" cos",
        ),
        (
            r"\mathbf{ab}\boldsymbol{\theta}\mathbb{R}\mathcal{A}",
            "bold(a b) bold(theta) bb(R) cal(A)",
        ),
        (r"\text{for all}", "\"for all\""),
        (
            r"\underline{x}\overline{x}\bar{x}\vec{x}\hat{x}\tilde{x}\dot{x}\ddot{x}",
            "underline(x) overline(x) overline(x) arrow(x) hat(x) tilde(x) dot(x) dot.double(x)",
        ),
        (
            r"\overset{a}{=}\underset{b}{x}\stackrel{c}{=}",
            "limits(=)^a limits(x)_b limits(=)^c",
        ),
        (
            r"a\,b\;c\:d\!e\quad f\qquad g~h",
            "a thin b thick c med d #h(-1em / 6) e quad f wide g space h",
        ),
        (r"x' + y\prime + z^{\prime}", "x' + y' + z'"),
        (r"\{x\}\mid y\|z", "{ x } divides y ‖ z"),
        (
            r"\begin{matrix}a&b\\c&d\end{matrix}",
            "mat(delim: #none, a, b; c, d)",
        ),
        (
            r"\begin{pmatrix}a&b\end{pmatrix}",
            "mat(delim: \"(\", a, b)",
        ),
        (
            r"\begin{bmatrix}a\\b\end{bmatrix}",
            "mat(delim: \"[\", a; b)",
        ),
        (
            r"\begin{array}{cc}a&b\\c&d\end{array}",
            "mat(delim: #none, a, b; c, d)",
        ),
        (
            r"\begin{cases}x&x>0\\0&x=0\end{cases}",
            "cases(x quad x > 0, 0 quad x = 0)",
        ),
        (
            r"\begin{aligned}x&=1\\y&=2\end{aligned}",
            r"x & = 1 \ y & = 2",
        ),
        (r"\sqrt{a/b;c}", r"sqrt(a \/ b \; c)"),
        (r"A\cap B=\varnothing", "A inter B = emptyset"),
        (
            r"\sum\limits_{k=1}^{n}a_k+\int\nolimits_0^1",
            "limits(sum)_(k = 1)^n a_k + scripts(integral)_0^1",
        ),
        (r"\Bigg\langle x\big)\Big|_{0}", r"⟨ x \) |_0"),
        (r"\frac{\big(a}{b\big)}", r"(\( a)/(b \))"),
        (r"\left[0,1\right)", r"lr(\[ 0 \, 1 \))"),
        (r"(a+b)^2+[0,1)", r"( a + b )^2 + \[ 0 , 1 \)"),
        (
            r"\frac{(}{x}+\sqrt{a)^2}+M(^{2}",
            r"(\()/(x) + sqrt(a \)^2) + M \(^2",
        ),
        (r"a\not=b\not\in C", "a eq.not b in.not C"),
        (r"\mathrm { d x }+\mathrm{m2}", r#""dx" + "m2""#),
        (r"\operatorname*{argmax}", r#"op("argmax", limits: #true)"#),
        (
            r"\textbf{Fall}\textit{a}\mbox{b}",
            r#"bold("Fall") italic("a") "b""#,
        ),
        (r"\dfrac{a}{b}+\binom{n}{k}", "(a)/(b) + binom(n, k)"),
        (r"a\equiv b\pmod{n}", "a equiv b (mod n)"),
        (r"\boxed{x=1}", "#box(stroke: 0.5pt, inset: 3pt, $x = 1$)"),
        (r"\sum_{\substack{i<n\\j>0}}a", r"sum_(i < n \ j > 0) a"),
        (
            r"\underbrace{a+b}_{n}\overbrace{c}^{2}\overbrace{d}",
            "underbrace(a + b, n) overbrace(c, 2) overbrace(d)",
        ),
        (
            r"\widehat{x}\check{y}\mathring{A}\mathfrak{g}\mathsf{T}\mathscr{L}",
            "hat(x) caron(y) circle(A) frak(g) sans(T) scr(L)",
        ),
        (r"\displaystyle x", "x"),
        (
            r"\begin{vmatrix}a&b\\c&d\end{vmatrix}\begin{Bmatrix}a\end{Bmatrix}",
            "mat(delim: \"|\", a, b; c, d) mat(delim: \"{\", a)",
        ),
        (
            r"\left\{\begin{array}{l}x=1\\y=2\end{array}\right.",
            r"lr(\{ mat(delim: #none, align: #left, x = 1; y = 2) )",
        ),
        (
            r"\begin{array}{rl}x&=1\\y&=2\end{array}",
            r"x & = 1 \ y & = 2",
        ),
        (
            r"\begin{rcases}a&b\end{rcases}",
            "cases(reverse: #true, a quad b)",
        ),
    ];
    fn convert(s: &str) -> String {
        latex_to_typst(s).unwrap_or_else(|e| panic!("{s}: {e}"))
    }
    #[test]
    fn maps_supported_constructs() {
        for (s, expected) in SAMPLES {
            assert_eq!(convert(s), *expected, "{s}");
        }
    }
    #[test]
    fn preserves_structure() {
        for (a, b) in [
            ("x_2", "x^2"),
            ("1.5", "15"),
            (r"\frac{a}{\frac{b}{c}}", r"\frac{\frac{a}{b}}{c}"),
            ("ab", r"\mathrm{ab}"),
        ] {
            assert_ne!(convert(a), convert(b));
        }
        assert_eq!(convert("x^12"), "x^1 2");
        assert_eq!(convert("\\text{  for\n all  }"), "\"for all\"");
    }
    #[test]
    fn refuses_unsupported_and_unbalanced_input() {
        for s in [
            r"\unknown{x}",
            "{x",
            "x}",
            r"\left(x",
            r"x\right)",
            "x&y",
            r"x\\y",
            r"\begin{unknown}x\end{unknown}",
            r"\begin{matrix}x\end{pmatrix}",
            r"\begin{array}{lr}a&b\end{array}",
            r"\begin{matrix}a&b\\c\end{matrix}",
            r"\text{a\foo}",
            "x_1_2",
            "x^",
            "",
            "x%comment",
            r"\input{file}",
            r"\begin{aligned}x&=1\end{aligned}^2",
            r"\hline",
            r"\not\to",
            r"\limits_0",
            r"\left\{\begin{array}{rl}a&b\end{array}\right.",
            r"\hat{}",
            r"\overset{}{=}",
            r"\text{$x$}",
            r"\mathrm{a+b}",
            r"\left.\right.",
        ] {
            assert!(latex_to_typst(s).is_err(), "{s}");
        }
    }
    #[test]
    fn bounds_resources() {
        assert!(latex_to_typst(&"x".repeat(16385)).is_err());
        assert!(latex_to_typst(&"x".repeat(4097)).is_err());
        assert!(latex_to_typst(&format!("{}x{}", "{".repeat(65), "}".repeat(65))).is_err());
    }
    #[test]
    fn converts_full_symbol_corpus() {
        for (input, _) in SAMPLES {
            assert_output(&convert(input));
        }
        for input in [
            r"\left\{ x \right\}",
            r"\left\langle x \right\rangle",
            r"\left\lfloor x \right\rfloor",
            r"\left\lceil x \right\rceil",
            r"\left/ x \right/",
            r"\left\| x \right\|",
            r"\left. x \right.",
        ] {
            assert_output(&convert(input));
        }
        for input in [
            r"\begin{matrix}a&\\b&c\end{matrix}",
            r#"\text{say "hello"}"#,
            r"\# \$ \% \& \_",
        ] {
            assert_output(&convert(input));
        }
        // Exercise every table entry, not only the common aliases above.
        for (name, _) in COMMAND_SYMBOLS {
            assert_output(&convert(&format!("\\{name}")));
        }
        for name in TYPST_OPERATORS {
            assert_output(&convert(&format!("\\{name}")));
        }
        for (name, _) in NEGATED_COMMANDS {
            assert_output(&convert(&format!("\\not\\{name}")));
        }
        for (symbol, _) in NEGATED_CHARS {
            assert_output(&convert(&format!("\\not{symbol}")));
        }
        for (name, _) in STYLE_FUNCTIONS {
            assert_output(&convert(&format!("\\{name}{{x}}")));
        }
        for (symbol, _) in CHAR_SYMBOLS {
            assert_output(&convert(&symbol.to_string()));
        }
    }
    fn assert_output(content: &str) {
        assert!(!content.trim().is_empty());
        assert!(content.len() + 4 <= 64 * 1024);
    }
}
