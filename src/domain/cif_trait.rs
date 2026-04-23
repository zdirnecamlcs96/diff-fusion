use serde::{Deserialize, Serialize};

/// Trait for converting any type to a Common Intermediate Format (CIF)
///
/// Implement this trait on your domain types to enable format-agnostic comparison.
///
/// # Examples
///
/// ```
/// use diff_fusion::CifSchema;
/// use serde::{Serialize, Deserialize};
///
/// #[derive(Debug, Serialize, Deserialize, PartialEq)]
/// struct ProductCif {
///     name: String,
///     price: f64,
/// }
///
/// struct SalesforceProduct {
///     Name: String,
///     Price__c: f64,
/// }
///
/// impl CifSchema for SalesforceProduct {
///     type Cif = ProductCif;
///
///     fn to_cif(&self) -> Result<Self::Cif, Box<dyn std::error::Error>> {
///         Ok(ProductCif {
///             name: self.Name.clone(),
///             price: self.Price__c,
///         })
///     }
/// }
/// ```
pub trait CifSchema {
    /// The Common Intermediate Format type
    type Cif: Serialize + for<'de> Deserialize<'de> + PartialEq;

    /// Convert this type to CIF
    fn to_cif(&self) -> Result<Self::Cif, Box<dyn std::error::Error>>;

    /// Optional: Validate the CIF against business rules
    fn validate_cif(cif: &Self::Cif) -> Result<(), String> {
        let _ = cif;
        Ok(())
    }
}

/// Compare two items that implement CifSchema
///
/// Returns true if they're equal after converting to CIF
pub fn compare_cif<T: CifSchema>(a: &T, b: &T) -> Result<bool, Box<dyn std::error::Error>> {
    let cif_a = a.to_cif()?;
    let cif_b = b.to_cif()?;
    Ok(cif_a == cif_b)
}

/// Get detailed differences between two CIF objects
pub fn diff_cif<T: CifSchema>(a: &T, b: &T) -> Result<Vec<String>, Box<dyn std::error::Error>>
where
    T::Cif: std::fmt::Debug,
{
    let cif_a = a.to_cif()?;
    let cif_b = b.to_cif()?;

    if cif_a == cif_b {
        return Ok(vec![]);
    }

    // Convert to JSON for detailed comparison
    let json_a = serde_json::to_value(&cif_a)?;
    let json_b = serde_json::to_value(&cif_b)?;

    let mut diffs = Vec::new();
    compare_json_values("", &json_a, &json_b, &mut diffs);

    Ok(diffs)
}

fn compare_json_values(
    path: &str,
    a: &serde_json::Value,
    b: &serde_json::Value,
    diffs: &mut Vec<String>,
) {
    use serde_json::Value;

    if a == b {
        return;
    }

    match (a, b) {
        (Value::Object(map_a), Value::Object(map_b)) => {
            use std::collections::HashSet;
            let keys: HashSet<_> = map_a.keys().chain(map_b.keys()).collect();

            for key in keys {
                let new_path = if path.is_empty() {
                    key.to_string()
                } else {
                    format!("{}.{}", path, key)
                };

                let val_a = map_a.get(key).unwrap_or(&Value::Null);
                let val_b = map_b.get(key).unwrap_or(&Value::Null);

                compare_json_values(&new_path, val_a, val_b, diffs);
            }
        }
        _ => {
            diffs.push(format!("{}: {:?} → {:?}", path, a, b));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct ProductCif {
        name: String,
        price: f64,
    }

    struct FormatA {
        name: String,
        price: f64,
    }

    struct FormatB {
        product_name: String,
        cost: f64,
    }

    impl CifSchema for FormatA {
        type Cif = ProductCif;

        fn to_cif(&self) -> Result<Self::Cif, Box<dyn std::error::Error>> {
            Ok(ProductCif {
                name: self.name.clone(),
                price: self.price,
            })
        }
    }

    impl CifSchema for FormatB {
        type Cif = ProductCif;

        fn to_cif(&self) -> Result<Self::Cif, Box<dyn std::error::Error>> {
            Ok(ProductCif {
                name: self.product_name.clone(),
                price: self.cost,
            })
        }
    }

    #[test]
    fn test_compare_same() {
        let a = FormatA {
            name: "Widget".to_string(),
            price: 19.99,
        };
        let b = FormatA {
            name: "Widget".to_string(),
            price: 19.99,
        };

        assert!(compare_cif(&a, &b).unwrap());
    }

    #[test]
    fn test_compare_different() {
        let a = FormatA {
            name: "Widget".to_string(),
            price: 19.99,
        };
        let b = FormatA {
            name: "Gadget".to_string(),
            price: 19.99,
        };

        assert!(!compare_cif(&a, &b).unwrap());
    }

    #[test]
    fn test_diff_details() {
        let a = FormatA {
            name: "Widget".to_string(),
            price: 19.99,
        };
        let b = FormatA {
            name: "Widget".to_string(),
            price: 24.99,
        };

        let diffs = diff_cif(&a, &b).unwrap();
        assert_eq!(diffs.len(), 1);
        assert!(diffs[0].contains("price"));
    }

    #[test]
    fn test_cross_format_comparison() {
        // To compare different formats, convert both to CIF first
        let format_a = FormatA {
            name: "Widget".to_string(),
            price: 19.99,
        };
        let format_b = FormatB {
            product_name: "Widget".to_string(),
            cost: 19.99,
        };

        let cif_a = format_a.to_cif().unwrap();
        let cif_b = format_b.to_cif().unwrap();

        assert_eq!(cif_a, cif_b);
    }
}
