//! Exact nonnegative rationals and named JSON distance matrices.
use crate::metric::Metric;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::fmt;
use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Rational {
    pub(crate) num: u128,
    pub(crate) den: u128,
}
fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}
impl Rational {
    pub(crate) fn new(num: u128, den: u128) -> Result<Self, String> {
        if den == 0 {
            return Err("a rational denominator must not be zero".into());
        }
        let divisor = gcd(num, den);
        Ok(Self {
            num: num / divisor,
            den: den / divisor,
        })
    }
    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        if text.is_empty() || text.starts_with('-') {
            return Err(format!("expected a nonnegative rational, got {text:?}"));
        }
        if let Some((a, b)) = text.split_once('/') {
            let a = a
                .trim()
                .parse::<u128>()
                .map_err(|_| format!("invalid fraction {text:?}"))?;
            let b = b
                .trim()
                .parse::<u128>()
                .map_err(|_| format!("invalid fraction {text:?}"))?;
            return Self::new(a, b);
        }
        let (mantissa, exponent) = match text.find(['e', 'E']) {
            Some(position) => (
                &text[..position],
                text[position + 1..]
                    .parse::<i32>()
                    .map_err(|_| "invalid decimal exponent")?,
            ),
            None => (text, 0),
        };
        let (integer, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        if integer.is_empty()
            || !integer
                .bytes()
                .chain(fraction.bytes())
                .all(|b| b.is_ascii_digit())
        {
            return Err(format!("invalid decimal {text:?}"));
        }
        let mut numerator = 0_u128;
        for digit in integer.bytes().chain(fraction.bytes()) {
            numerator = numerator
                .checked_mul(10)
                .and_then(|n| n.checked_add((digit - b'0') as u128))
                .ok_or("rational numerator exceeds u128")?;
        }
        if numerator == 0 {
            return Self::new(0, 1);
        }
        let decimals =
            i64::try_from(fraction.len()).map_err(|_| "decimal is too long")? - i64::from(exponent);
        let power =
            u32::try_from(decimals.unsigned_abs()).map_err(|_| "decimal exponent is too large")?;
        let factor = 10_u128
            .checked_pow(power)
            .ok_or("decimal exponent exceeds exact arithmetic range")?;
        if decimals >= 0 {
            Self::new(numerator, factor)
        } else {
            Self::new(
                numerator
                    .checked_mul(factor)
                    .ok_or("rational numerator exceeds u128")?,
                1,
            )
        }
    }
    pub(crate) fn multiple(self, value: u32) -> Result<Self, String> {
        let divisor = gcd(value as u128, self.den);
        Self::new(
            self.num
                .checked_mul(value as u128 / divisor)
                .ok_or("result exceeds exact arithmetic range")?,
            self.den / divisor,
        )
    }
    pub(crate) fn half_multiple(self, value: u32) -> Result<Self, String> {
        let result = self.multiple(value)?;
        if result.num.is_multiple_of(2) {
            Self::new(result.num / 2, result.den)
        } else {
            Self::new(
                result.num,
                result
                    .den
                    .checked_mul(2)
                    .ok_or("result denominator exceeds u128")?,
            )
        }
    }
}
impl fmt::Display for Rational {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.den == 1 {
            write!(f, "{}", self.num)
        } else {
            write!(f, "{}/{}", self.num, self.den)
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Space {
    pub(crate) name: String,
    pub(crate) labels: Vec<String>,
    pub(crate) distances: Vec<Rational>,
}
impl Space {
    pub(crate) fn read(path: &Path) -> Result<Self, String> {
        let value = read_json(path)?;
        Self::from_json(
            &value,
            &path.file_stem().unwrap_or_default().to_string_lossy(),
        )
        .map_err(|error| format!("{}: {error}", path.display()))
    }
    pub(crate) fn from_json(value: &Value, default_name: &str) -> Result<Self, String> {
        let object = value.as_object().ok_or("a space must be a JSON object")?;
        for key in object.keys() {
            if !["name", "points", "distances"].contains(&key.as_str()) {
                return Err(format!("unknown space field {key:?}"));
            }
        }
        let name = match object.get("name") {
            Some(v) => v.as_str().ok_or("name must be a string")?.to_string(),
            None => default_name.into(),
        };
        let rows = object
            .get("distances")
            .and_then(Value::as_array)
            .ok_or("distances must be a square matrix")?;
        let order = rows.len();
        if order == 0 {
            return Err("an empty space is not supported; use [[0]] for a singleton".into());
        }
        if order > 1024 {
            return Err("input matrix exceeds the 1024-point input limit".into());
        }
        let labels = match object.get("points") {
            Some(v) => v
                .as_array()
                .ok_or("points must be an array of names")?
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| "point names must be strings".to_string())
                })
                .collect::<Result<Vec<_>, _>>()?,
            None => (0..order).map(|i| i.to_string()).collect(),
        };
        if labels.len() != order {
            return Err("points and distances have different sizes".into());
        }
        if labels.iter().any(String::is_empty)
            || labels.iter().collect::<HashSet<_>>().len() != order
        {
            return Err("point names must be nonempty and unique".into());
        }
        let mut distances = Vec::with_capacity(order * order);
        for (i, row) in rows.iter().enumerate() {
            let row = row
                .as_array()
                .ok_or_else(|| format!("row {i} is not an array"))?;
            if row.len() != order {
                return Err(format!(
                    "row {i} has {} entries; expected {order}",
                    row.len()
                ));
            }
            for (j, v) in row.iter().enumerate() {
                let text = match v {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    _ => {
                        return Err(format!(
                            "distance ({i},{j}) must be a number or rational string"
                        ));
                    }
                };
                distances.push(Rational::parse(&text).map_err(|e| {
                    format!(
                        "distance ({i},{j}) between {:?} and {:?}: {e}",
                        labels[i], labels[j]
                    )
                })?);
            }
        }
        for i in 0..order {
            for j in 0..order {
                let d = distances[i * order + j];
                if (i == j && d.num != 0) || (i != j && d.num == 0) {
                    return Err(format!(
                        "invalid distance at ({i},{j}) between {:?} and {:?}: {d}",
                        labels[i], labels[j]
                    ));
                }
                if d != distances[j * order + i] {
                    return Err(format!("matrix is not symmetric at ({i},{j})"));
                }
            }
        }
        let space = Self {
            name,
            labels,
            distances,
        };
        normalize(&space, &space)?;
        Ok(space)
    }
    pub(crate) fn to_json(&self) -> Value {
        let order = self.labels.len();
        let rows: Vec<Vec<_>> = self
            .distances
            .chunks(order)
            .map(|row| row.iter().map(ToString::to_string).collect())
            .collect();
        json!({"name":self.name,"points":self.labels,"distances":rows})
    }
}
pub(crate) fn read_json(path: &Path) -> Result<Value, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if file.metadata().map_err(|e| e.to_string())?.len() > 64 * 1024 * 1024 {
        return Err(format!("{}: JSON input exceeds 64 MiB", path.display()));
    }
    serde_json::from_reader(file).map_err(|e| format!("{}: {e}", path.display()))
}
pub(crate) fn normalize(left: &Space, right: &Space) -> Result<(Metric, Metric, Rational), String> {
    let mut denominator = 1_u128;
    for value in left.distances.iter().chain(&right.distances) {
        denominator = (denominator / gcd(denominator, value.den))
            .checked_mul(value.den)
            .ok_or("common denominator exceeds u128")?;
    }
    let integers: Vec<u128> = left
        .distances
        .iter()
        .chain(&right.distances)
        .map(|v| {
            v.num
                .checked_mul(denominator / v.den)
                .ok_or_else(|| "scaled distance exceeds u128".to_string())
        })
        .collect::<Result<_, _>>()?;
    let divisor = integers.iter().copied().fold(0, gcd).max(1);
    let entries: Vec<u32> = integers
        .into_iter()
        .map(|v| {
            u32::try_from(v / divisor).map_err(|_| {
                "distance ratios exceed the engine's u32 range after exact normalization"
                    .to_string()
            })
        })
        .collect::<Result<_, _>>()?;
    let a = Metric {
        order: left.labels.len(),
        entries: entries[..left.distances.len()].to_vec(),
    };
    let b = Metric {
        order: right.labels.len(),
        entries: entries[left.distances.len()..].to_vec(),
    };
    a.validate()
        .map_err(|e| format!("{}: {e}; point order {:?}", left.name, left.labels))?;
    b.validate()
        .map_err(|e| format!("{}: {e}; point order {:?}", right.name, right.labels))?;
    Ok((a, b, Rational::new(divisor, denominator)?))
}
