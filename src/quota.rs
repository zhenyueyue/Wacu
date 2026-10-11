//! Shared quota window display semantics for subscription providers.
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindow {
    pub used_percent: Option<f64>,
    pub window_duration_mins: Option<i64>,
    pub resets_at: Option<i64>,
}

impl QuotaWindow {
    pub fn percent(&self) -> Option<f64> {
        self.used_percent
            .filter(|v| v.is_finite())
            .map(|v| v.clamp(0.0, 100.0))
    }

    pub fn label(&self) -> String {
        match self.window_duration_mins {
            Some(10080) => "每周额度".into(),
            Some(n) if n > 0 && n % 1440 == 0 => format!("{}天额度", n / 1440),
            Some(n) if n > 0 && n % 60 == 0 => format!("{}小时额度", n / 60),
            Some(n) if n > 0 => format!("{n}分钟额度"),
            _ => "额度窗口".into(),
        }
    }

    pub fn summary(&self) -> String {
        self.percent()
            .map(|p| format!("已用 {p:.1}% · 剩余 {:.1}%", 100.0 - p))
            .unwrap_or_else(|| "额度暂不可用".into())
    }

    pub fn reset(&self) -> String {
        self.resets_at
            .and_then(DateTime::from_timestamp_secs)
            .map(|date| date.with_timezone(&Local).format("%m/%d %H:%M").to_string())
            .unwrap_or_else(|| "未提供重置时间".into())
    }
}
