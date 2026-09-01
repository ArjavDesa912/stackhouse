//! Pure audience-rule evaluation.
//!
//! Rules are AND-combined. Unknown fields or operators fail closed: a config
//! typo must never accidentally expose an experiment to everyone.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};
#[cfg(test)]
use serde_json::json;
use serde_json::Value;

/// One audience rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub field: String,
    pub op: String,
    #[serde(default)]
    pub value: Option<Value>,
}

/// Request-supplied + stored customer context.
#[derive(Debug, Clone)]
pub struct AudienceContext<'a> {
    pub country: Option<&'a str>,
    pub app_version: Option<&'a str>,
    pub is_existing_subscriber: bool,
    pub attributes: &'a Value,
}

/// Evaluate a JSON array of rules against `ctx`.
///
/// Returns `false` if the rule list is malformed, the field is unknown, or the
/// operator is unsupported — this is the fail-closed behaviour required for
/// audience gating.
pub fn is_eligible(rules: &Value, ctx: &AudienceContext) -> bool {
    let rules: Vec<Rule> = match serde_json::from_value(rules.clone()) {
        Ok(r) => r,
        Err(_) => return false,
    };

    if rules.is_empty() {
        return true;
    }

    rules.iter().all(|rule| eval_rule(rule, ctx))
}

fn eval_rule(rule: &Rule, ctx: &AudienceContext) -> bool {
    match rule.op.as_str() {
        "exists" => field_exists(&rule.field, ctx),
        _ => {
            let Some(actual) = get_field_value(&rule.field, ctx) else {
                return false;
            };
            match rule.op.as_str() {
                "eq" => rule.value.as_ref() == Some(&actual),
                "neq" => rule.value.as_ref() != Some(&actual),
                "gt" => compare_values(&actual, rule.value.as_ref()) == Some(Ordering::Greater),
                "gte" => {
                    let ord = compare_values(&actual, rule.value.as_ref());
                    ord == Some(Ordering::Greater) || ord == Some(Ordering::Equal)
                }
                "lt" => compare_values(&actual, rule.value.as_ref()) == Some(Ordering::Less),
                "lte" => {
                    let ord = compare_values(&actual, rule.value.as_ref());
                    ord == Some(Ordering::Less) || ord == Some(Ordering::Equal)
                }
                "in" => in_array(&actual, rule.value.as_ref(), true),
                "not_in" => !in_array(&actual, rule.value.as_ref(), true),
                _ => false, // unknown operator -> fail closed
            }
        }
    }
}

fn get_field_value(field: &str, ctx: &AudienceContext) -> Option<Value> {
    match field {
        "country" => ctx.country.map(|s| Value::String(s.to_string())),
        "app_version" => ctx.app_version.map(|s| Value::String(s.to_string())),
        "is_existing_subscriber" => Some(Value::Bool(ctx.is_existing_subscriber)),
        _ => {
            if let Some(key) = field.strip_prefix("attributes.") {
                ctx.attributes.get(key).cloned()
            } else {
                None
            }
        }
    }
}

fn field_exists(field: &str, ctx: &AudienceContext) -> bool {
    if let Some(key) = field.strip_prefix("attributes.") {
        ctx.attributes.get(key).is_some()
    } else {
        get_field_value(field, ctx).is_some()
    }
}

fn in_array(actual: &Value, rule_value: Option<&Value>, required: bool) -> bool {
    let Some(rule_value) = rule_value else {
        return false;
    };
    if let Some(arr) = rule_value.as_array() {
        arr.iter().any(|item| item == actual)
    } else if required {
        false
    } else {
        rule_value == actual
    }
}

fn compare_values(left: &Value, right: Option<&Value>) -> Option<Ordering> {
    let right = right?;

    // Numeric comparison when both sides parse as numbers.
    if let (Some(l), Some(r)) = (as_number(left), as_number(right)) {
        return l.partial_cmp(&r);
    }

    // Version-aware comparison for app_version.
    if let (Some(l), Some(r)) = (left.as_str(), right.as_str()) {
        return Some(compare_semverish(l, r));
    }

    // Fallback: stringify. This is only reached for non-numeric, non-string
    // values (e.g. bool vs bool), where lexicographic JSON is sufficient.
    left.to_string().partial_cmp(&right.to_string())
}

fn as_number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

/// Compare two dot-separated version-ish strings numerically segment by segment.
fn compare_semverish(left: &str, right: &str) -> Ordering {
    let parse = |s: &str| {
        s.split('.')
            .map(|p| p.parse::<u64>().unwrap_or(0))
            .collect::<Vec<_>>()
    };
    let l = parse(left);
    let r = parse(right);
    l.cmp(&r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(att: Value) -> AudienceContext<'static> {
        AudienceContext {
            country: Some("US"),
            app_version: Some("1.10.0"),
            is_existing_subscriber: false,
            attributes: Box::leak(Box::new(att)),
        }
    }

    #[test]
    fn simple_country_match() {
        let rules = json!([{ "field": "country", "op": "eq", "value": "US" }]);
        assert!(is_eligible(&rules, &ctx(json!({"plan":"free"}))));
    }

    #[test]
    fn version_greater_than_works() {
        let rules = json!([{ "field": "app_version", "op": "gte", "value": "1.9.0" }]);
        assert!(is_eligible(&rules, &ctx(json!({}))));
    }

    #[test]
    fn version_less_than_fails() {
        let rules = json!([{ "field": "app_version", "op": "lt", "value": "1.2.0" }]);
        assert!(!is_eligible(&rules, &ctx(json!({}))));
    }

    #[test]
    fn attribute_rule_and_combo() {
        let rules = json!([
            { "field": "country", "op": "eq", "value": "US" },
            { "field": "attributes.plan", "op": "eq", "value": "pro" }
        ]);
        assert!(is_eligible(&rules, &ctx(json!({"plan":"pro"}))));
        assert!(!is_eligible(&rules, &ctx(json!({"plan":"free"}))));
    }

    #[test]
    fn unknown_operator_fails_closed() {
        let rules = json!([{ "field": "country", "op": "typo", "value": "US" }]);
        assert!(!is_eligible(&rules, &ctx(json!({}))));
    }

    #[test]
    fn unknown_field_fails_closed() {
        let rules = json!([{ "field": "device_model", "op": "eq", "value": "iPhone" }]);
        assert!(!is_eligible(&rules, &ctx(json!({}))));
    }

    #[test]
    fn exists_op_for_missing_attribute() {
        let rules = json!([{ "field": "attributes.feature_flag", "op": "exists" }]);
        assert!(!is_eligible(&rules, &ctx(json!({}))));
        assert!(is_eligible(&rules, &ctx(json!({"feature_flag": true}))));
    }

    #[test]
    fn in_operator_uses_array() {
        let rules = json!([{ "field": "country", "op": "in", "value": ["US","CA"] }]);
        assert!(is_eligible(&rules, &ctx(json!({}))));
        let rules2 = json!([{ "field": "country", "op": "in", "value": ["DE"] }]);
        assert!(!is_eligible(&rules2, &ctx(json!({}))));
    }
}
