//! Pure experiment logic.
//!
//! - Deterministic variant assignment via sha256 hashing.
//! - Two-proportion z-test for rough confidence vs. control.

use sha2::{Digest, Sha256};

use super::models::Variant;

/// Deterministically assign a customer to one of `variants` for `experiment_id`.
///
/// Returns the chosen variant's `id`, or `None` if there are no variants.
/// The bucket space is `[0, 10000)`; `traffic_weight` is interpreted as an
/// integer percentage (0-100), so `weight * 100` basis points are allocated.
///
/// The algorithm is stable: the same `(experiment_id, customer_id)` pair always
/// lands on the same variant as long as the variant ids/weights are unchanged.
pub fn assign_variant(experiment_id: i64, customer_id: i64, variants: &[Variant]) -> Option<i64> {
    if variants.is_empty() {
        return None;
    }

    let mut hasher = Sha256::new();
    hasher.update(experiment_id.to_le_bytes());
    hasher.update(customer_id.to_le_bytes());
    let digest = hasher.finalize();

    let bucket = u64::from_le_bytes(digest[0..8].try_into().unwrap()) % 10_000;

    // Sort by id for a stable, deterministic walk.
    let mut ordered = variants.to_vec();
    ordered.sort_by(|a, b| a.id.cmp(&b.id));

    let mut cumulative_bp: i64 = 0;
    for variant in &ordered {
        cumulative_bp += i64::from(variant.traffic_weight) * 100;
        if bucket < cumulative_bp as u64 {
            return Some(variant.id);
        }
    }

    // If the weights don't sum to exactly 100%, fall back to the last variant
    // rather than returning nothing. The caller is expected to validate weights.
    ordered.last().map(|v| v.id)
}

/// Per-variant counts used by the confidence calculator.
#[derive(Debug, Clone, Copy)]
pub struct VariantCounts {
    pub impressions: i64,
    pub conversions: i64,
}

/// Two-proportion z-test for `variant` vs. `control`.
///
/// Returns `None` if either group has zero impressions or the standard error
/// is zero (no measurable difference possible). This is an estimate, not a
/// rigorous statistical conclusion.
pub fn confidence_vs_control(control: VariantCounts, variant: VariantCounts) -> Option<f64> {
    let c_n = control.impressions as f64;
    let v_n = variant.impressions as f64;
    if c_n <= 0.0 || v_n <= 0.0 {
        return None;
    }

    let c_p = control.conversions as f64 / c_n;
    let v_p = variant.conversions as f64 / v_n;

    let pooled_p = (control.conversions as f64 + variant.conversions as f64) / (c_n + v_n);
    if pooled_p <= 0.0 || pooled_p >= 1.0 {
        return None;
    }

    let se = (pooled_p * (1.0 - pooled_p) * (1.0 / c_n + 1.0 / v_n)).sqrt();
    if se == 0.0 {
        return None;
    }

    Some((v_p - c_p) / se)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_variant(id: i64, weight: i32) -> Variant {
        Variant {
            id,
            experiment_id: 1,
            identifier: format!("v{id}"),
            offering_id: 100 + id,
            is_control: id == 1,
            traffic_weight: weight,
        }
    }

    #[test]
    fn assignment_is_deterministic() {
        let variants = vec![mk_variant(1, 50), mk_variant(2, 50)];
        let a = assign_variant(1, 42, &variants);
        let b = assign_variant(1, 42, &variants);
        assert_eq!(a, b);
    }

    #[test]
    fn assignment_respects_rough_weight_proportions() {
        let variants = vec![mk_variant(1, 75), mk_variant(2, 25)];
        let mut v1 = 0;
        let mut v2 = 0;
        for i in 0..1000 {
            match assign_variant(7, i, &variants) {
                Some(1) => v1 += 1,
                Some(2) => v2 += 1,
                _ => {}
            }
        }
        // Allow 10% tolerance; the exact split is deterministic but should be close.
        assert!(v1 > 650 && v1 < 850, "expected ~75% in variant 1, got {v1}");
        assert!(v2 > 150 && v2 < 350, "expected ~25% in variant 2, got {v2}");
    }

    #[test]
    fn z_test_on_known_values() {
        let control = VariantCounts {
            impressions: 1000,
            conversions: 100,
        };
        let variant = VariantCounts {
            impressions: 1000,
            conversions: 150,
        };
        let z = confidence_vs_control(control, variant).unwrap();
        // A 5% absolute lift on 1000/1000 impressions should be around z=2.3
        assert!(z > 1.5 && z < 3.5, "unexpected z {z}");
    }

    #[test]
    fn z_test_returns_none_for_zero_impressions() {
        assert!(confidence_vs_control(
            VariantCounts {
                impressions: 0,
                conversions: 0
            },
            VariantCounts {
                impressions: 100,
                conversions: 10
            }
        )
        .is_none());
    }
}
