//! Sample summaries and release-budget checks. Platform independent.

use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Summary {
    pub count: usize,
    pub min: f64,
    pub median: f64,
    pub p95: f64,
    pub max: f64,
    pub mean: f64,
}

/// Nearest-rank percentile (`0.0 < q <= 1.0`) of sorted samples.
pub fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let rank = (q * sorted.len() as f64).ceil().max(1.0) as usize;
    sorted[rank.min(sorted.len()) - 1]
}

pub fn summarize(samples: &[f64]) -> Option<Summary> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    Some(Summary {
        count: sorted.len(),
        min: sorted[0],
        median: percentile(&sorted, 0.5),
        p95: percentile(&sorted, 0.95),
        max: sorted[sorted.len() - 1],
        mean: sorted.iter().sum::<f64>() / sorted.len() as f64,
    })
}

/// Named metrics plus raw samples for one run.
#[derive(Default)]
pub struct Results {
    pub metrics: BTreeMap<String, f64>,
    pub samples: BTreeMap<String, Vec<f64>>,
    pub notes: Vec<String>,
}

impl Results {
    pub fn metric(&mut self, name: impl Into<String>, value: f64) {
        self.metrics.insert(name.into(), value);
    }

    /// Stores raw samples and `<name>.{median,p95,max,mean,n}` metrics.
    pub fn samples(&mut self, name: &str, samples: Vec<f64>) {
        if let Some(summary) = summarize(&samples) {
            self.metric(format!("{name}.median"), summary.median);
            self.metric(format!("{name}.p95"), summary.p95);
            self.metric(format!("{name}.max"), summary.max);
            self.metric(format!("{name}.mean"), summary.mean);
            self.metric(format!("{name}.n"), summary.count as f64);
        }
        self.samples.insert(name.to_owned(), samples);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Comparison {
    AtMost,
    AtLeast,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Budget {
    pub metric: String,
    pub comparison: Comparison,
    pub limit: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BudgetOutcome {
    Pass(f64),
    Fail(f64),
    /// The run did not produce the metric (scenario skipped or broken).
    Missing,
}

/// Parses `metric <= limit` / `metric >= limit` lines; `#` starts a comment.
pub fn parse_budgets(text: &str) -> Result<Vec<Budget>, String> {
    let mut budgets = Vec::new();
    for (number, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let (metric, comparison, limit) = if let Some((metric, limit)) = line.split_once("<=") {
            (metric, Comparison::AtMost, limit)
        } else if let Some((metric, limit)) = line.split_once(">=") {
            (metric, Comparison::AtLeast, limit)
        } else {
            return Err(format!("line {}: expected `metric <= limit`", number + 1));
        };
        let limit: f64 = limit
            .trim()
            .parse()
            .map_err(|_| format!("line {}: invalid limit", number + 1))?;
        let metric = metric.trim();
        if metric.is_empty() || metric.contains(char::is_whitespace) {
            return Err(format!("line {}: invalid metric name", number + 1));
        }
        budgets.push(Budget {
            metric: metric.to_owned(),
            comparison,
            limit,
        });
    }
    Ok(budgets)
}

pub fn check(budget: &Budget, metrics: &BTreeMap<String, f64>) -> BudgetOutcome {
    match metrics.get(&budget.metric) {
        None => BudgetOutcome::Missing,
        Some(&value) => {
            let ok = match budget.comparison {
                Comparison::AtMost => value <= budget.limit,
                Comparison::AtLeast => value >= budget.limit,
            };
            if ok {
                BudgetOutcome::Pass(value)
            } else {
                BudgetOutcome::Fail(value)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_rank_percentiles() {
        let samples: Vec<f64> = (1..=20).map(f64::from).collect();
        let summary = summarize(&samples).unwrap();
        assert_eq!(summary.median, 10.0);
        assert_eq!(summary.p95, 19.0);
        assert_eq!(summary.max, 20.0);
        assert_eq!(summarize(&[3.0]).unwrap().p95, 3.0);
        assert!(summarize(&[]).is_none());
    }

    #[test]
    fn budgets_parse_and_check() {
        let budgets = parse_budgets(
            "# comment\nresize.present_ms.p95 <= 16.5\n\nidle.paints >= 0 # trailing\n",
        )
        .unwrap();
        assert_eq!(budgets.len(), 2);
        assert_eq!(budgets[0].comparison, Comparison::AtMost);
        let mut metrics = BTreeMap::new();
        metrics.insert("resize.present_ms.p95".to_string(), 20.0);
        assert_eq!(check(&budgets[0], &metrics), BudgetOutcome::Fail(20.0));
        assert_eq!(check(&budgets[1], &metrics), BudgetOutcome::Missing);
        assert!(parse_budgets("nonsense").is_err());
        assert!(parse_budgets("a <= x").is_err());
    }
}
