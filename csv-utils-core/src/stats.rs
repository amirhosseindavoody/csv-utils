#[derive(Debug, Clone, Default)]
pub struct ColStats {
    pub rows: usize,
    pub nulls: usize,
    pub non_nulls: usize,
    pub max_width: usize,
}

#[derive(Debug)]
pub struct StatsAgg {
    cols: Vec<ColStats>,
}

impl StatsAgg {
    pub fn new(col_count: usize) -> Self {
        Self {
            cols: vec![ColStats::default(); col_count],
        }
    }

    pub fn observe(&mut self, fields: &[String]) {
        let limit = fields.len().min(self.cols.len());
        for (i, value) in fields.iter().take(limit).enumerate() {
            self.cols[i].rows += 1;
            if value.is_empty() {
                self.cols[i].nulls += 1;
            } else {
                self.cols[i].non_nulls += 1;
                self.cols[i].max_width = self.cols[i].max_width.max(value.len());
            }
        }
    }

    pub fn print(&self, headers: &[String]) -> String {
        let mut out = String::new();
        for (i, c) in self.cols.iter().enumerate() {
            let name = headers.get(i).map(String::as_str).unwrap_or("unknown");
            out.push_str(&format!(
                "{name}: rows={} nulls={} non_nulls={} max_width={}\n",
                c.rows, c.nulls, c.non_nulls, c.max_width
            ));
        }
        out
    }
}
