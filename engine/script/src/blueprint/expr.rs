//! Formula text as nodes: `(strength + vitality * 8) * 3` becomes Add and Multiply nodes
//! wired as the text says, so a formula written as text (by an importer, an agent or a person
//! pasting one) is an ordinary graph to read and edit. Numbers, `+ - * / %`, comparisons
//! (`< <= > >= == !=`), `&& || !`, brackets and `min(a, b)`, `max(a, b)`, `if(c, a, b)`,
//! `random(low, high)`; any other name is asked of the caller (a field of a record, a
//! variable...).

use super::model::{Graph, GraphNode, Literal, PinRef, Wire};

/// A value in a formula: a number, or what an output pin gives.
#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    Number(f64),
    Pin { node: u32, pin: String },
}

/// Answers a name a formula uses (`args` empty), or a function the writer doesn't know
/// (with its values): what it gives, or `None` when there is none.
pub type Resolve<'r> = dyn FnMut(&mut ExprWriter, &str, &[Operand]) -> Option<Operand> + 'r;

/// Writes formulas into `graph`, placing nodes from `origin` to the right.
pub struct ExprWriter<'a> {
    pub graph: &'a mut Graph,
    origin: [f32; 2],
    placed: usize,
}

/// What the formula is made of.
#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Name(String),
    Op(&'static str),
    Open,
    Close,
    Comma,
}

const OPS: [&str; 15] = [
    "<=", ">=", "==", "!=", "&&", "||", "+", "-", "*", "/", "%", "<", ">", "!", "?",
];

fn tokens(text: &str) -> Result<Vec<Token>, String> {
    let mut out = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit()
            || (c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit))
        {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            out.push(Token::Number(
                text.parse()
                    .map_err(|_| format!("\"{text}\" isn't a number"))?,
            ));
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.')
            {
                i += 1;
            }
            out.push(Token::Name(chars[start..i].iter().collect()));
        } else if c == '(' {
            out.push(Token::Open);
            i += 1;
        } else if c == ')' {
            out.push(Token::Close);
            i += 1;
        } else if c == ',' {
            out.push(Token::Comma);
            i += 1;
        } else {
            let rest: String = chars[i..chars.len().min(i + 2)].iter().collect();
            let op = (OPS.iter())
                .find(|op| rest.starts_with(**op))
                .ok_or_else(|| format!("\"{c}\" can't be in a formula"))?;
            out.push(Token::Op(op));
            i += op.len();
        }
    }
    Ok(out)
}

/// A formula being read: its tokens and where reading is.
struct Reader<'t> {
    tokens: &'t [Token],
    at: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.at).cloned();
        self.at += 1;
        token
    }

    fn eat_op(&mut self, ops: &[&'static str]) -> Option<&'static str> {
        match self.peek() {
            Some(Token::Op(op)) if ops.contains(op) => {
                let op = *op;
                self.at += 1;
                Some(op)
            }
            _ => None,
        }
    }
}

impl<'a> ExprWriter<'a> {
    pub fn new(graph: &'a mut Graph, origin: [f32; 2]) -> Self {
        Self {
            graph,
            origin,
            placed: 0,
        }
    }

    /// Writes `text` as nodes, `name` answering each name it uses; what it gives.
    pub fn write(&mut self, text: &str, name: &mut Resolve<'_>) -> Result<Operand, String> {
        let tokens = tokens(text)?;
        let mut reader = Reader {
            tokens: &tokens,
            at: 0,
        };
        let value = self.or(&mut reader, name)?;
        match reader.next() {
            None => Ok(value),
            Some(t) => Err(format!("unexpected {t:?} in \"{text}\"")),
        }
    }

    /// Adds node `def` with `values`; its id.
    pub fn node(&mut self, def: &str, values: &[(&str, Literal)]) -> u32 {
        let id = self.graph.next_id();
        let pos = [
            self.origin[0] + (self.placed % 6) as f32 * 200.0,
            self.origin[1] + (self.placed / 6) as f32 * 110.0,
        ];
        self.placed += 1;
        self.graph.nodes.push(GraphNode {
            id,
            def: def.to_owned(),
            pos,
            values: (values.iter())
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect(),
        });
        id
    }

    /// Puts `value` into input `pin` of node `to`: a number as the pin's value, a pin by a wire.
    pub fn feed(&mut self, value: &Operand, to: u32, pin: &str) {
        match value {
            Operand::Number(n) => {
                if let Some(node) = self.graph.nodes.iter_mut().find(|n| n.id == to) {
                    node.values.insert(pin.to_owned(), Literal::Number(*n));
                }
            }
            Operand::Pin { node, pin: out } => self.graph.wires.push(Wire {
                from: PinRef {
                    node: *node,
                    pin: out.clone(),
                },
                to: PinRef {
                    node: to,
                    pin: pin.to_owned(),
                },
            }),
        }
    }

    /// Pure node `def` of `inputs`; its answer.
    fn pure(&mut self, def: &str, inputs: &[(&str, Operand)]) -> Operand {
        let node = self.node(def, &[]);
        for (pin, value) in inputs {
            self.feed(value, node, pin);
        }
        Operand::Pin {
            node,
            pin: "value".into(),
        }
    }

    fn binary(&mut self, op: &str, a: Operand, b: Operand) -> Operand {
        if let (Operand::Number(x), Operand::Number(y)) = (&a, &b) {
            let folded = match op {
                "+" => Some(x + y),
                "-" => Some(x - y),
                "*" => Some(x * y),
                "/" if *y != 0.0 => Some((x / y).trunc()),
                "%" if *y != 0.0 => Some(x % y),
                _ => None,
            };
            if let Some(n) = folded {
                return Operand::Number(n);
            }
        }
        let not = |w: &mut Self, v: Operand| w.pure("logic.not", &[("a", v)]);
        match op {
            "+" => self.pure("math.add", &[("a", a), ("b", b)]),
            "-" => self.pure("math.subtract", &[("a", a), ("b", b)]),
            "*" => self.pure("math.multiply", &[("a", a), ("b", b)]),
            "/" => self.pure("math.divide", &[("a", a), ("b", b)]),
            "%" => self.pure("math.remainder", &[("a", a), ("b", b)]),
            "<" => self.pure("logic.less", &[("a", a), ("b", b)]),
            ">" => self.pure("logic.greater", &[("a", a), ("b", b)]),
            "==" => self.pure("logic.equal", &[("a", a), ("b", b)]),
            "<=" => {
                let gt = self.pure("logic.greater", &[("a", a), ("b", b)]);
                not(self, gt)
            }
            ">=" => {
                let lt = self.pure("logic.less", &[("a", a), ("b", b)]);
                not(self, lt)
            }
            "!=" => {
                let eq = self.pure("logic.equal", &[("a", a), ("b", b)]);
                not(self, eq)
            }
            "&&" => self.pure("logic.and", &[("a", a), ("b", b)]),
            _ => self.pure("logic.or", &[("a", a), ("b", b)]),
        }
    }

    fn or(&mut self, r: &mut Reader, name: &mut Resolve<'_>) -> Result<Operand, String> {
        let mut value = self.and(r, name)?;
        while r.eat_op(&["||"]).is_some() {
            let rhs = self.and(r, name)?;
            value = self.binary("||", value, rhs);
        }
        Ok(value)
    }

    fn and(&mut self, r: &mut Reader, name: &mut Resolve<'_>) -> Result<Operand, String> {
        let mut value = self.compare(r, name)?;
        while r.eat_op(&["&&"]).is_some() {
            let rhs = self.compare(r, name)?;
            value = self.binary("&&", value, rhs);
        }
        Ok(value)
    }

    fn compare(&mut self, r: &mut Reader, name: &mut Resolve<'_>) -> Result<Operand, String> {
        let value = self.sum(r, name)?;
        match r.eat_op(&["<", ">", "<=", ">=", "==", "!="]) {
            Some(op) => {
                let rhs = self.sum(r, name)?;
                Ok(self.binary(op, value, rhs))
            }
            None => Ok(value),
        }
    }

    fn sum(&mut self, r: &mut Reader, name: &mut Resolve<'_>) -> Result<Operand, String> {
        let mut value = self.term(r, name)?;
        while let Some(op) = r.eat_op(&["+", "-"]) {
            let rhs = self.term(r, name)?;
            value = self.binary(op, value, rhs);
        }
        Ok(value)
    }

    fn term(&mut self, r: &mut Reader, name: &mut Resolve<'_>) -> Result<Operand, String> {
        let mut value = self.unary(r, name)?;
        while let Some(op) = r.eat_op(&["*", "/", "%"]) {
            let rhs = self.unary(r, name)?;
            value = self.binary(op, value, rhs);
        }
        Ok(value)
    }

    fn unary(&mut self, r: &mut Reader, name: &mut Resolve<'_>) -> Result<Operand, String> {
        match r.eat_op(&["!", "-"]) {
            Some("!") => {
                let v = self.unary(r, name)?;
                Ok(self.pure("logic.not", &[("a", v)]))
            }
            Some(_) => {
                let v = self.unary(r, name)?;
                Ok(self.binary("-", Operand::Number(0.0), v))
            }
            None => self.primary(r, name),
        }
    }

    fn primary(&mut self, r: &mut Reader, name: &mut Resolve<'_>) -> Result<Operand, String> {
        match r.next() {
            Some(Token::Number(n)) => Ok(Operand::Number(n)),
            Some(Token::Open) => {
                let v = self.or(r, name)?;
                match r.next() {
                    Some(Token::Close) => Ok(v),
                    _ => Err("a bracket isn't closed".into()),
                }
            }
            Some(Token::Name(id)) if r.peek() == Some(&Token::Open) => {
                r.next();
                let mut args = Vec::new();
                if r.peek() != Some(&Token::Close) {
                    loop {
                        args.push(self.or(r, name)?);
                        match r.next() {
                            Some(Token::Comma) => continue,
                            Some(Token::Close) => break,
                            _ => return Err(format!("{id}(...) isn't closed")),
                        }
                    }
                } else {
                    r.next();
                }
                match self.call(&id, args.clone()) {
                    Ok(value) => Ok(value),
                    Err(e) => name(self, &id, &args).ok_or(e),
                }
            }
            Some(Token::Name(id)) => {
                name(self, &id, &[]).ok_or_else(|| format!("nothing is called \"{id}\""))
            }
            other => Err(format!("a value was expected, not {other:?}")),
        }
    }

    fn call(&mut self, function: &str, mut args: Vec<Operand>) -> Result<Operand, String> {
        let want = match function {
            "min" | "max" | "random" => 2,
            "if" => 3,
            _ => return Err(format!("there is no function \"{function}\"")),
        };
        if args.len() != want {
            return Err(format!("{function}() takes {want} values"));
        }
        let b = args.pop().expect("counted");
        let a = args.pop().expect("counted");
        Ok(match function {
            "min" => self.pure("math.min", &[("a", a), ("b", b)]),
            "max" => self.pure("math.max", &[("a", a), ("b", b)]),
            "random" => self.pure("math.random", &[("low", a), ("high", b)]),
            _ => {
                let cond = args.pop().expect("counted");
                self.pure("logic.select", &[("condition", cond), ("a", a), ("b", b)])
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blueprint::{Blueprint, compile};

    #[test]
    fn a_formula_becomes_nodes_that_compile() {
        let mut bp = Blueprint::default();
        let event = GraphNode {
            id: 1,
            def: "formula.level_exp".into(),
            pos: [0.0, 0.0],
            values: Default::default(),
        };
        bp.events.nodes.push(event);
        let mut w = ExprWriter::new(&mut bp.events, [200.0, 0.0]);
        let mut names = |_: &mut ExprWriter, n: &str, _: &[Operand]| {
            (n == "level").then(|| Operand::Pin {
                node: 1,
                pin: "level".into(),
            })
        };
        let value = w
            .write(
                "if(level < 10, level * 100, max(level * 2 + 3 * 4, 50)) - -1",
                &mut names,
            )
            .unwrap();
        let ret = w.node("flow.return", &[]);
        w.feed(&value, ret, "value");
        w.graph.wires.push(Wire {
            from: PinRef {
                node: 1,
                pin: "then".into(),
            },
            to: PinRef {
                node: ret,
                pin: "exec".into(),
            },
        });
        let script = compile("t", &bp).expect("compiles");
        assert!(script.contains("level"), "{script}");
        let mut w = ExprWriter::new(&mut bp.events, [0.0, 0.0]);
        assert!(w.write("2 + nothing", &mut names).is_err());
        assert_eq!(
            w.write("(1 + 2) * 4 / 3", &mut names).unwrap(),
            Operand::Number(4.0)
        );
    }
}
