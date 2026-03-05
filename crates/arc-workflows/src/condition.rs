/// Condition expression evaluator for edge guards (spec Section 10).
///
/// Grammar:
/// ```text
/// Expr       ::= OrExpr
/// OrExpr     ::= AndExpr ('||' AndExpr)*
/// AndExpr    ::= UnaryExpr ('&&' UnaryExpr)*
/// UnaryExpr  ::= '!' UnaryExpr | Clause
/// Clause     ::= Key Op Literal | Key        (bare key = truthy)
/// Op         ::= '=' | '!=' | '>' | '<' | '>=' | '<='
///              | 'contains' | 'matches'
/// ```
use crate::context::Context;
use crate::error::ArcError;
use crate::outcome::Outcome;

// ---------------------------------------------------------------------------
// AST
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum ConditionExpr {
    Clause(Clause),
    Not(Box<ConditionExpr>),
    And(Vec<ConditionExpr>),
    Or(Vec<ConditionExpr>),
}

#[derive(Debug, Clone, PartialEq)]
struct Clause {
    key: String,
    op: Op,
    value: String,
}

#[derive(Debug, Clone, PartialEq)]
enum Op {
    Eq,
    NotEq,
    Gt,
    Lt,
    Gte,
    Lte,
    Contains,
    Matches,
    Truthy,
}

// ---------------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Word(String),
    OpEq,       // =
    OpNotEq,    // !=
    OpGt,       // >
    OpLt,       // <
    OpGte,      // >=
    OpLte,      // <=
    And,        // &&
    Or,         // ||
    Not,        // !
    Contains,   // contains
    Matches,    // matches
}

fn tokenize(input: &str) -> Result<Vec<Token>, ArcError> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(Vec::new());
    }

    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut tokens = Vec::new();

    while i < len {
        // Skip whitespace
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }

        // Two-char operators (longest match first)
        if i + 1 < len {
            let two = format!("{}{}", chars[i], chars[i + 1]);
            match two.as_str() {
                "&&" => { tokens.push(Token::And); i += 2; continue; }
                "||" => { tokens.push(Token::Or); i += 2; continue; }
                "!=" => { tokens.push(Token::OpNotEq); i += 2; continue; }
                ">=" => { tokens.push(Token::OpGte); i += 2; continue; }
                "<=" => { tokens.push(Token::OpLte); i += 2; continue; }
                _ => {}
            }
        }

        // Single-char operators
        match chars[i] {
            '=' => { tokens.push(Token::OpEq); i += 1; continue; }
            '>' => { tokens.push(Token::OpGt); i += 1; continue; }
            '<' => { tokens.push(Token::OpLt); i += 1; continue; }
            '!' => { tokens.push(Token::Not); i += 1; continue; }
            _ => {}
        }

        // Word: everything up to whitespace or operator char
        let start = i;
        while i < len && !chars[i].is_whitespace() && !is_op_char(chars[i]) {
            i += 1;
        }
        if i == start {
            return Err(ArcError::Parse(format!(
                "unexpected character '{}' in condition expression",
                chars[i]
            )));
        }
        let word: String = chars[start..i].iter().collect();

        // Recognize keyword operators only when they appear between words
        // (not as the first or last token, and not adjacent to another operator)
        match word.as_str() {
            "contains" if is_word_operator_context(&tokens) => {
                tokens.push(Token::Contains);
            }
            "matches" if is_word_operator_context(&tokens) => {
                tokens.push(Token::Matches);
            }
            _ => {
                tokens.push(Token::Word(word));
            }
        }
    }

    Ok(tokens)
}

fn is_op_char(c: char) -> bool {
    matches!(c, '=' | '!' | '>' | '<' | '&' | '|')
}

/// Word operators (`contains`, `matches`) are recognized when preceded by a Word token.
fn is_word_operator_context(tokens: &[Token]) -> bool {
    matches!(tokens.last(), Some(Token::Word(_)))
}

// ---------------------------------------------------------------------------
// Parser (recursive descent)
// ---------------------------------------------------------------------------

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<Token> {
        let tok = self.tokens.get(self.pos).cloned();
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    fn parse_expr(&mut self) -> Result<ConditionExpr, ArcError> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<ConditionExpr, ArcError> {
        let mut children = vec![self.parse_and()?];
        while self.peek() == Some(&Token::Or) {
            self.advance();
            children.push(self.parse_and()?);
        }
        if children.len() == 1 {
            Ok(children.pop().expect("just checked length"))
        } else {
            Ok(ConditionExpr::Or(children))
        }
    }

    fn parse_and(&mut self) -> Result<ConditionExpr, ArcError> {
        let mut children = vec![self.parse_unary()?];
        while self.peek() == Some(&Token::And) {
            self.advance();
            children.push(self.parse_unary()?);
        }
        if children.len() == 1 {
            Ok(children.pop().expect("just checked length"))
        } else {
            Ok(ConditionExpr::And(children))
        }
    }

    fn parse_unary(&mut self) -> Result<ConditionExpr, ArcError> {
        if self.peek() == Some(&Token::Not) {
            self.advance();
            let inner = self.parse_unary()?;
            return Ok(ConditionExpr::Not(Box::new(inner)));
        }
        self.parse_clause()
    }

    fn parse_clause(&mut self) -> Result<ConditionExpr, ArcError> {
        let key = match self.advance() {
            Some(Token::Word(w)) => w,
            Some(other) => {
                return Err(ArcError::Parse(format!(
                    "expected key, got {other:?} in condition expression"
                )));
            }
            None => {
                return Err(ArcError::Parse(
                    "unexpected end of condition expression".to_string(),
                ));
            }
        };

        // Check for operator
        let op = match self.peek() {
            Some(Token::OpEq) => Some(Op::Eq),
            Some(Token::OpNotEq) => Some(Op::NotEq),
            Some(Token::OpGt) => Some(Op::Gt),
            Some(Token::OpLt) => Some(Op::Lt),
            Some(Token::OpGte) => Some(Op::Gte),
            Some(Token::OpLte) => Some(Op::Lte),
            Some(Token::Contains) => Some(Op::Contains),
            Some(Token::Matches) => Some(Op::Matches),
            _ => None,
        };

        let Some(op) = op else {
            // Bare key → truthy
            return Ok(ConditionExpr::Clause(Clause {
                key,
                op: Op::Truthy,
                value: String::new(),
            }));
        };

        self.advance(); // consume the operator

        // Value: must be a Word
        let value = match self.advance() {
            Some(Token::Word(w)) => w,
            Some(other) => {
                return Err(ArcError::Parse(format!(
                    "expected value after operator, got {other:?}"
                )));
            }
            None => {
                // Allow empty value for `=` and `!=` (backward compat: `missing_key=`)
                if op == Op::Eq || op == Op::NotEq {
                    String::new()
                } else {
                    return Err(ArcError::Parse(
                        "expected value after operator".to_string(),
                    ));
                }
            }
        };

        // Validate regex at parse time
        if op == Op::Matches {
            regex::Regex::new(&value).map_err(|e| {
                ArcError::Parse(format!("invalid regex pattern '{value}': {e}"))
            })?;
        }

        Ok(ConditionExpr::Clause(Clause { key, op, value }))
    }
}

fn parse_expression(expr: &str) -> Result<ConditionExpr, ArcError> {
    let tokens = tokenize(expr)?;
    if tokens.is_empty() {
        return Ok(ConditionExpr::And(Vec::new()));
    }
    let mut parser = Parser::new(tokens);
    let result = parser.parse_expr()?;
    if parser.pos < parser.tokens.len() {
        return Err(ArcError::Parse(format!(
            "unexpected token {:?} in condition expression",
            parser.tokens[parser.pos]
        )));
    }
    Ok(result)
}

/// Parse and validate a condition expression.
///
/// # Errors
///
/// Returns an error if the expression contains invalid syntax.
pub fn parse_condition(expr: &str) -> Result<(), ArcError> {
    parse_expression(expr)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Evaluator
// ---------------------------------------------------------------------------

fn resolve_key(key: &str, outcome: &Outcome, context: &Context) -> String {
    if key == "outcome" {
        return outcome.status.to_string();
    }
    if key == "preferred_label" {
        return outcome.preferred_label.as_deref().unwrap_or("").to_string();
    }
    if let Some(path) = key.strip_prefix("context.") {
        if let Some(val) = context.get(key) {
            return json_value_to_string(&val);
        }
        if let Some(val) = context.get(path) {
            return json_value_to_string(&val);
        }
        return String::new();
    }
    context
        .get(key)
        .map_or_else(String::new, |val| json_value_to_string(&val))
}

fn resolve_key_value(
    key: &str,
    outcome: &Outcome,
    context: &Context,
) -> serde_json::Value {
    if key == "outcome" {
        return serde_json::Value::String(outcome.status.to_string());
    }
    if key == "preferred_label" {
        return outcome
            .preferred_label
            .as_deref()
            .map_or(serde_json::Value::Null, |s| {
                serde_json::Value::String(s.to_string())
            });
    }
    if let Some(path) = key.strip_prefix("context.") {
        if let Some(val) = context.get(key) {
            return val;
        }
        if let Some(val) = context.get(path) {
            return val;
        }
        return serde_json::Value::Null;
    }
    context.get(key).unwrap_or(serde_json::Value::Null)
}

fn json_value_to_string(val: &serde_json::Value) -> String {
    match val {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn is_truthy(s: &str) -> bool {
    !s.is_empty() && s != "false" && s != "0"
}

fn eval_expr(expr: &ConditionExpr, outcome: &Outcome, context: &Context) -> bool {
    match expr {
        ConditionExpr::And(children) => {
            if children.is_empty() {
                return true;
            }
            children.iter().all(|c| eval_expr(c, outcome, context))
        }
        ConditionExpr::Or(children) => {
            children.iter().any(|c| eval_expr(c, outcome, context))
        }
        ConditionExpr::Not(inner) => !eval_expr(inner, outcome, context),
        ConditionExpr::Clause(clause) => eval_clause(clause, outcome, context),
    }
}

fn eval_clause(clause: &Clause, outcome: &Outcome, context: &Context) -> bool {
    match &clause.op {
        Op::Truthy => {
            let resolved = resolve_key(&clause.key, outcome, context);
            is_truthy(&resolved)
        }
        Op::Eq => {
            let resolved = resolve_key(&clause.key, outcome, context);
            resolved == clause.value
        }
        Op::NotEq => {
            let resolved = resolve_key(&clause.key, outcome, context);
            resolved != clause.value
        }
        Op::Gt | Op::Lt | Op::Gte | Op::Lte => {
            let resolved = resolve_key(&clause.key, outcome, context);
            let lhs: f64 = match resolved.parse() {
                Ok(v) => v,
                Err(_) => return false,
            };
            let rhs: f64 = match clause.value.parse() {
                Ok(v) => v,
                Err(_) => return false,
            };
            match &clause.op {
                Op::Gt => lhs > rhs,
                Op::Lt => lhs < rhs,
                Op::Gte => lhs >= rhs,
                Op::Lte => lhs <= rhs,
                _ => unreachable!(),
            }
        }
        Op::Contains => {
            let raw = resolve_key_value(&clause.key, outcome, context);
            match &raw {
                serde_json::Value::Array(arr) => arr.iter().any(|elem| {
                    json_value_to_string(elem) == clause.value
                }),
                _ => {
                    let s = json_value_to_string(&raw);
                    s.contains(&clause.value)
                }
            }
        }
        Op::Matches => {
            let resolved = resolve_key(&clause.key, outcome, context);
            // Regex was validated at parse time, so unwrap is safe
            regex::Regex::new(&clause.value)
                .map(|re| re.is_match(&resolved))
                .unwrap_or(false)
        }
    }
}

/// Evaluate a condition expression against an outcome and context.
/// Empty conditions always return true.
#[must_use]
pub fn evaluate_condition(expr: &str, outcome: &Outcome, context: &Context) -> bool {
    let Ok(parsed) = parse_expression(expr) else {
        return false;
    };
    eval_expr(&parsed, outcome, context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::StageStatus;

    fn make_outcome(status: StageStatus) -> Outcome {
        Outcome {
            status,
            ..Outcome::success()
        }
    }

    // -----------------------------------------------------------------------
    // Phase 0: Existing behavior preserved
    // -----------------------------------------------------------------------

    #[test]
    fn empty_condition_is_true() {
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        assert!(evaluate_condition("", &outcome, &context));
        assert!(evaluate_condition("  ", &outcome, &context));
    }

    #[test]
    fn outcome_equals_success() {
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        assert!(evaluate_condition("outcome=success", &outcome, &context));
        assert!(!evaluate_condition("outcome=fail", &outcome, &context));
    }

    #[test]
    fn outcome_not_equals() {
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        assert!(evaluate_condition("outcome!=fail", &outcome, &context));
        assert!(!evaluate_condition("outcome!=success", &outcome, &context));
    }

    #[test]
    fn preferred_label_match() {
        let mut outcome = make_outcome(StageStatus::Success);
        outcome.preferred_label = Some("Fix".to_string());
        let context = Context::new();
        assert!(evaluate_condition(
            "preferred_label=Fix",
            &outcome,
            &context
        ));
        assert!(!evaluate_condition(
            "preferred_label=Approve",
            &outcome,
            &context
        ));
    }

    #[test]
    fn context_key_with_prefix() {
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        context.set("tests_passed", serde_json::json!("true"));
        assert!(evaluate_condition(
            "context.tests_passed=true",
            &outcome,
            &context
        ));
    }

    #[test]
    fn bare_key_context_lookup() {
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        context.set("custom_key", serde_json::json!("custom_value"));
        assert!(evaluate_condition(
            "custom_key=custom_value",
            &outcome,
            &context
        ));
    }

    #[test]
    fn missing_key_compares_as_empty() {
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        assert!(!evaluate_condition(
            "missing_key=something",
            &outcome,
            &context
        ));
        assert!(evaluate_condition("missing_key=", &outcome, &context));
    }

    #[test]
    fn multiple_clauses_and() {
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        context.set("tests_passed", serde_json::json!("true"));
        assert!(evaluate_condition(
            "outcome=success && context.tests_passed=true",
            &outcome,
            &context
        ));
        assert!(!evaluate_condition(
            "outcome=fail && context.tests_passed=true",
            &outcome,
            &context
        ));
    }

    #[test]
    fn parse_condition_validates() {
        assert!(parse_condition("outcome=success").is_ok());
        assert!(parse_condition("outcome=success && context.x=y").is_ok());
        assert!(parse_condition("").is_ok());
    }

    #[test]
    fn parse_condition_accepts_bare_key() {
        assert!(parse_condition("some_flag").is_ok());
    }

    #[test]
    fn context_dotted_fallback() {
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        context.set("loop_state", serde_json::json!("exhausted"));
        assert!(evaluate_condition(
            "context.loop_state=exhausted",
            &outcome,
            &context
        ));
    }

    #[test]
    fn bare_key_truthy_when_non_empty() {
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        context.set("my_flag", serde_json::json!("yes"));
        assert!(evaluate_condition("my_flag", &outcome, &context));
    }

    #[test]
    fn bare_key_falsy_when_empty() {
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        assert!(!evaluate_condition("missing_key", &outcome, &context));
    }

    #[test]
    fn bare_key_falsy_when_false_string() {
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        context.set("my_flag", serde_json::json!("false"));
        assert!(!evaluate_condition("my_flag", &outcome, &context));
    }

    #[test]
    fn bare_key_falsy_when_zero_string() {
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        context.set("my_flag", serde_json::json!("0"));
        assert!(!evaluate_condition("my_flag", &outcome, &context));
    }

    #[test]
    fn bare_key_with_and_clause() {
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        context.set("flag", serde_json::json!("yes"));
        assert!(evaluate_condition(
            "outcome=success && flag",
            &outcome,
            &context
        ));
    }

    #[test]
    fn context_failure_class_matches_when_set() {
        let outcome = make_outcome(StageStatus::Fail);
        let context = Context::new();
        context.set("failure_class", serde_json::json!("budget_exhausted"));
        assert!(evaluate_condition(
            "context.failure_class=budget_exhausted",
            &outcome,
            &context
        ));
    }

    #[test]
    fn context_failure_class_not_equals_on_success() {
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        context.set("failure_class", serde_json::json!(""));
        assert!(evaluate_condition(
            "context.failure_class!=transient_infra",
            &outcome,
            &context
        ));
    }

    #[test]
    fn context_failure_class_combined_with_outcome() {
        let outcome = make_outcome(StageStatus::Fail);
        let context = Context::new();
        context.set("failure_class", serde_json::json!("transient_infra"));
        assert!(evaluate_condition(
            "outcome=fail && context.failure_class=transient_infra",
            &outcome,
            &context
        ));
        assert!(!evaluate_condition(
            "outcome=fail && context.failure_class=deterministic",
            &outcome,
            &context
        ));
    }

    // -----------------------------------------------------------------------
    // Phase 0: AST structure tests
    // -----------------------------------------------------------------------

    #[test]
    fn parse_eq_into_clause() {
        let expr = parse_expression("outcome=success").unwrap();
        assert_eq!(
            expr,
            ConditionExpr::Clause(Clause {
                key: "outcome".to_string(),
                op: Op::Eq,
                value: "success".to_string(),
            })
        );
    }

    #[test]
    fn parse_and_into_and_node() {
        let expr = parse_expression("a=1 && b=2").unwrap();
        assert_eq!(
            expr,
            ConditionExpr::And(vec![
                ConditionExpr::Clause(Clause {
                    key: "a".to_string(),
                    op: Op::Eq,
                    value: "1".to_string(),
                }),
                ConditionExpr::Clause(Clause {
                    key: "b".to_string(),
                    op: Op::Eq,
                    value: "2".to_string(),
                }),
            ])
        );
    }

    #[test]
    fn parse_bare_key_into_truthy() {
        let expr = parse_expression("some_flag").unwrap();
        assert_eq!(
            expr,
            ConditionExpr::Clause(Clause {
                key: "some_flag".to_string(),
                op: Op::Truthy,
                value: String::new(),
            })
        );
    }

    #[test]
    fn parse_not_eq_into_clause() {
        let expr = parse_expression("outcome!=fail").unwrap();
        assert_eq!(
            expr,
            ConditionExpr::Clause(Clause {
                key: "outcome".to_string(),
                op: Op::NotEq,
                value: "fail".to_string(),
            })
        );
    }

    // -----------------------------------------------------------------------
    // Phase 1: Numeric comparisons
    // -----------------------------------------------------------------------

    #[test]
    fn numeric_gt() {
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        context.set("score", serde_json::json!(90));
        assert!(evaluate_condition("context.score > 80", &outcome, &context));
        context.set("score", serde_json::json!(70));
        assert!(!evaluate_condition("context.score > 80", &outcome, &context));
    }

    #[test]
    fn numeric_gte() {
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        context.set("score", serde_json::json!(80));
        assert!(evaluate_condition("context.score >= 80", &outcome, &context));
    }

    #[test]
    fn numeric_lte() {
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        context.set("score", serde_json::json!(80));
        assert!(evaluate_condition("context.score <= 80", &outcome, &context));
    }

    #[test]
    fn numeric_lt() {
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        context.set("count", serde_json::json!(3));
        assert!(evaluate_condition("context.count < 5", &outcome, &context));
    }

    #[test]
    fn numeric_float() {
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        context.set("ratio", serde_json::json!(0.75));
        assert!(evaluate_condition(
            "context.ratio > 0.5",
            &outcome,
            &context
        ));
    }

    #[test]
    fn numeric_non_numeric_returns_false() {
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        context.set("score", serde_json::json!("not_a_number"));
        assert!(!evaluate_condition(
            "context.score > 80",
            &outcome,
            &context
        ));
    }

    #[test]
    fn parse_numeric_comparisons() {
        assert!(parse_condition("x > 5").is_ok());
        assert!(parse_condition("x >= 5").is_ok());
        assert!(parse_condition("x < 5").is_ok());
        assert!(parse_condition("x <= 5").is_ok());
    }

    // -----------------------------------------------------------------------
    // Phase 2: contains operator
    // -----------------------------------------------------------------------

    #[test]
    fn contains_substring() {
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        context.set("message", serde_json::json!("an error occurred"));
        assert!(evaluate_condition(
            "context.message contains error",
            &outcome,
            &context
        ));
        context.set("message", serde_json::json!("all good"));
        assert!(!evaluate_condition(
            "context.message contains error",
            &outcome,
            &context
        ));
    }

    #[test]
    fn contains_case_sensitive() {
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        context.set("message", serde_json::json!("an error occurred"));
        assert!(!evaluate_condition(
            "context.message contains Error",
            &outcome,
            &context
        ));
    }

    #[test]
    fn contains_json_array() {
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        context.set("tags", serde_json::json!(["urgent", "low"]));
        assert!(evaluate_condition(
            "context.tags contains urgent",
            &outcome,
            &context
        ));
        assert!(!evaluate_condition(
            "context.tags contains critical",
            &outcome,
            &context
        ));
    }

    #[test]
    fn parse_contains() {
        assert!(parse_condition("x contains y").is_ok());
    }

    // -----------------------------------------------------------------------
    // Phase 3: matches operator (regex)
    // -----------------------------------------------------------------------

    #[test]
    fn matches_regex() {
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        context.set("version", serde_json::json!("v2.0"));
        assert!(evaluate_condition(
            r"context.version matches ^v\d+",
            &outcome,
            &context
        ));
        context.set("version", serde_json::json!("beta"));
        assert!(!evaluate_condition(
            r"context.version matches ^v\d+",
            &outcome,
            &context
        ));
    }

    #[test]
    fn matches_invalid_regex_fails_parse() {
        assert!(parse_condition("x matches [bad").is_err());
    }

    #[test]
    fn parse_matches() {
        assert!(parse_condition("x matches ^ok$").is_ok());
    }

    // -----------------------------------------------------------------------
    // Phase 4: OR (||)
    // -----------------------------------------------------------------------

    #[test]
    fn or_disjunction() {
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        assert!(evaluate_condition(
            "outcome=success || outcome=partial_success",
            &outcome,
            &context
        ));
        let outcome = make_outcome(StageStatus::Fail);
        assert!(!evaluate_condition(
            "outcome=success || outcome=partial_success",
            &outcome,
            &context
        ));
    }

    #[test]
    fn or_precedence_and_binds_tighter() {
        // a=1 && b=2 || c=3  is  (a=1 AND b=2) OR c=3
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        context.set("a", serde_json::json!("0"));
        context.set("b", serde_json::json!("2"));
        context.set("c", serde_json::json!("3"));
        // a=1 is false, b=2 is true => AND is false; c=3 is true => OR is true
        assert!(evaluate_condition(
            "a=1 && b=2 || c=3",
            &outcome,
            &context
        ));
    }

    #[test]
    fn or_precedence_right_and() {
        // a=1 || b=2 && c=3  is  a=1 OR (b=2 AND c=3)
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        context.set("a", serde_json::json!("0"));
        context.set("b", serde_json::json!("2"));
        context.set("c", serde_json::json!("0"));
        // a=1 false; b=2 true, c=3 false => AND false; OR false
        assert!(!evaluate_condition(
            "a=1 || b=2 && c=3",
            &outcome,
            &context
        ));
    }

    #[test]
    fn parse_or() {
        assert!(parse_condition("a=1 || b=2").is_ok());
    }

    // -----------------------------------------------------------------------
    // Phase 5: NOT (!)
    // -----------------------------------------------------------------------

    #[test]
    fn not_negation() {
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        assert!(evaluate_condition("!outcome=fail", &outcome, &context));
        assert!(!evaluate_condition("!outcome=success", &outcome, &context));
    }

    #[test]
    fn not_missing_key_is_true() {
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        assert!(evaluate_condition("!missing_key", &outcome, &context));
    }

    #[test]
    fn not_with_and() {
        let outcome = make_outcome(StageStatus::Success);
        let context = Context::new();
        context.set("ready", serde_json::json!("true"));
        assert!(evaluate_condition(
            "!outcome=fail && context.ready=true",
            &outcome,
            &context
        ));
    }

    #[test]
    fn parse_not() {
        assert!(parse_condition("!x=y").is_ok());
    }
}
