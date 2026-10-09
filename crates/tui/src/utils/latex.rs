//! 基础数学表达式转为终端 Unicode；未知命令或不完整语法返回 None，由调用方保留源码。
pub fn render_latex(source: &str) -> Option<String> {
    if source
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return None;
    }
    let mut parser = Parser {
        chars: source.chars().collect(),
        position: 0,
        depth: 0,
    };
    parser.sequence(None)
}
struct Parser {
    chars: Vec<char>,
    position: usize,
    depth: usize,
}
impl Parser {
    fn sequence(&mut self, end: Option<char>) -> Option<String> {
        if self.depth >= 64 {
            return None;
        }
        self.depth += 1;
        let mut result = String::new();
        while let Some(ch) = self.chars.get(self.position).copied() {
            if Some(ch) == end {
                self.position += 1;
                self.depth -= 1;
                return Some(result);
            }
            if ch == '}' {
                return None;
            }
            if matches!(ch, '^' | '_') {
                self.position += 1;
                let argument = self.argument()?;
                result.push_str(&script(&argument, ch));
            } else {
                result.push_str(&self.atom()?);
            }
        }
        self.depth -= 1;
        if end.is_some() { None } else { Some(result) }
    }
    fn argument(&mut self) -> Option<String> {
        while self
            .chars
            .get(self.position)
            .is_some_and(|c| c.is_whitespace())
        {
            self.position += 1;
        }
        self.atom()
    }
    fn atom(&mut self) -> Option<String> {
        if self.depth >= 64 {
            return None;
        }
        self.depth += 1;
        let ch = *self.chars.get(self.position)?;
        self.position += 1;
        let result = match ch {
            '{' => self.sequence(Some('}')),
            '}' => None,
            '\\' => self.command(),
            '~' => Some(" ".into()),
            '=' | '<' | '>' => Some(format!(" {ch} ")),
            c => Some(c.to_string()),
        };
        self.depth -= 1;
        result
    }
    fn command(&mut self) -> Option<String> {
        let start = self.position;
        while self
            .chars
            .get(self.position)
            .is_some_and(|c| c.is_ascii_alphabetic())
        {
            self.position += 1;
        }
        if start == self.position {
            let ch = *self.chars.get(self.position)?;
            self.position += 1;
            return match ch {
                ',' | ';' | ':' | ' ' => Some(" ".into()),
                '!' => Some(String::new()),
                '\\' => Some("\n".into()),
                '{' | '}' | '%' | '$' | '#' | '_' | '&' => Some(ch.to_string()),
                _ => None,
            };
        }
        let command: String = self.chars[start..self.position].iter().collect();
        if let Some(value) = symbol(&command) {
            return Some(value.into());
        }
        match command.as_str() {
            "frac" | "dfrac" | "tfrac" => {
                let numerator = self.argument()?;
                let denominator = self.argument()?;
                Some(format!("({numerator})/({denominator})"))
            }
            "sqrt" => {
                let mut root = String::new();
                if self.chars.get(self.position) == Some(&'[') {
                    self.position += 1;
                    root = script(&self.sequence(Some(']'))?, '^');
                }
                Some(format!("{root}√({})", self.argument()?))
            }
            "text" | "mathrm" | "mathbf" | "mathit" | "mathcal" | "mathsf" | "mathtt"
            | "operatorname" | "textbf" | "textit" | "mbox" => self.argument(),
            "mathbb" => {
                let value = self.argument()?;
                Some(
                    value
                        .chars()
                        .map(|ch| match ch {
                            'C' => 'ℂ',
                            'H' => 'ℍ',
                            'N' => 'ℕ',
                            'P' => 'ℙ',
                            'Q' => 'ℚ',
                            'R' => 'ℝ',
                            'Z' => 'ℤ',
                            c => c,
                        })
                        .collect(),
                )
            }
            "left" | "right" | "big" | "Big" | "bigg" | "Bigg" => {
                let value = self.argument()?;
                Some(if value == "." { String::new() } else { value })
            }
            "displaystyle" | "textstyle" | "scriptstyle" | "scriptscriptstyle" | "limits"
            | "nolimits" => Some(String::new()),
            "quad" | "qquad" | "enspace" | "thinspace" => Some(" ".into()),
            "sin" | "cos" | "tan" | "cot" | "sec" | "csc" | "log" | "ln" | "exp" | "lim"
            | "min" | "max" | "det" | "gcd" | "sup" | "inf" => Some(format!(" {command} ")),
            "hat" | "bar" | "overline" | "tilde" | "vec" | "dot" | "ddot" | "underline" => {
                let argument = self.argument()?;
                let accent = match command.as_str() {
                    "hat" => '\u{0302}',
                    "bar" | "overline" => '\u{0305}',
                    "tilde" => '\u{0303}',
                    "vec" => '\u{20d7}',
                    "dot" => '\u{0307}',
                    "ddot" => '\u{0308}',
                    _ => '\u{0332}',
                };
                Some(argument.chars().flat_map(|ch| [ch, accent]).collect())
            }
            _ => None,
        }
    }
}
fn script(value: &str, kind: char) -> String {
    let source = "0123456789+-=()abcdefghijklmnopqrstuvwxyz";
    let target = if kind == '^' {
        "⁰¹²³⁴⁵⁶⁷⁸⁹⁺⁻⁼⁽⁾ᵃᵇᶜᵈᵉᶠᵍʰⁱʲᵏˡᵐⁿᵒᵖ?ʳˢᵗᵘᵛʷˣʸᶻ"
    } else {
        "₀₁₂₃₄₅₆₇₈₉₊₋₌₍₎ₐ???ₑ??ₕᵢⱼₖₗₘₙₒₚ?ᵣₛₜᵤᵥ?ₓ??"
    };
    let mut result = String::new();
    for ch in value.chars() {
        let Some(index) = source.chars().position(|c| c == ch) else {
            return format!("{kind}({value})");
        };
        let mapped = target.chars().nth(index).unwrap_or('?');
        if mapped == '?' {
            return format!("{kind}({value})");
        }
        result.push(mapped);
    }
    result
}
fn symbol(command: &str) -> Option<&'static str> {
    Some(match command {
        "alpha" => "α",
        "beta" => "β",
        "gamma" => "γ",
        "delta" => "δ",
        "epsilon" => "ϵ",
        "varepsilon" => "ε",
        "zeta" => "ζ",
        "eta" => "η",
        "theta" => "θ",
        "iota" => "ι",
        "kappa" => "κ",
        "lambda" => "λ",
        "mu" => "μ",
        "nu" => "ν",
        "xi" => "ξ",
        "pi" => "π",
        "rho" => "ρ",
        "sigma" => "σ",
        "tau" => "τ",
        "upsilon" => "υ",
        "phi" => "ϕ",
        "varphi" => "φ",
        "chi" => "χ",
        "psi" => "ψ",
        "omega" => "ω",
        "Gamma" => "Γ",
        "Delta" => "Δ",
        "Theta" => "Θ",
        "Lambda" => "Λ",
        "Xi" => "Ξ",
        "Pi" => "Π",
        "Sigma" => "Σ",
        "Phi" => "Φ",
        "Psi" => "Ψ",
        "Omega" => "Ω",
        "pm" => "±",
        "mp" => "∓",
        "times" => "×",
        "div" => "÷",
        "cdot" => "·",
        "sum" => "∑",
        "prod" => "∏",
        "int" => "∫",
        "iint" => "∬",
        "oint" => "∮",
        "infty" => "∞",
        "partial" => "∂",
        "nabla" => "∇",
        "le" | "leq" => "≤",
        "ge" | "geq" => "≥",
        "ne" | "neq" => "≠",
        "approx" => "≈",
        "equiv" => "≡",
        "in" => "∈",
        "notin" => "∉",
        "subset" => "⊂",
        "subseteq" => "⊆",
        "supset" => "⊃",
        "supseteq" => "⊇",
        "cup" => "∪",
        "cap" => "∩",
        "forall" => "∀",
        "exists" => "∃",
        "neg" => "¬",
        "land" => "∧",
        "lor" => "∨",
        "to" | "rightarrow" => "→",
        "leftarrow" => "←",
        "leftrightarrow" => "↔",
        "Rightarrow" => "⇒",
        "Leftarrow" => "⇐",
        "Leftrightarrow" => "⇔",
        "ldots" | "dots" => "…",
        "cdots" => "⋯",
        "vdots" => "⋮",
        "ddots" => "⋱",
        "langle" => "⟨",
        "rangle" => "⟩",
        "lvert" | "rvert" | "vert" => "│",
        "lVert" | "rVert" | "Vert" => "║",
        _ => return None,
    })
}
