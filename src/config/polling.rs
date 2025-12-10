use duration_str::deserialize_duration;
use std::time::Duration;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct PollingConfig {
    #[serde(deserialize_with = "deserialize_duration")]
    pub(crate) initial_wait: Duration,

    #[serde(deserialize_with = "deserialize_duration")]
    pub(crate) poll_interval: Duration,

    #[serde(deserialize_with = "deserialize_duration")]
    pub(crate) max_wait: Duration,

    #[serde(deserialize_with = "deserialize_duration")]
    pub(crate) error_wait: Duration,

    pub(crate) max_errors: u64,
}

impl Default for PollingConfig {
    fn default() -> Self {
        Self {
            initial_wait: Duration::from_secs(10),
            poll_interval: Duration::from_secs(5),
            max_wait: Duration::from_secs(600),
            error_wait: Duration::from_secs(10),
            max_errors: 5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize, Debug, PartialEq)]
    struct A {
        #[serde(default)]
        config: PollingConfig,
    }

    #[test]
    fn test_polling_config_deserialize_defaults() {
        let yaml_data = r#"
          config:
            poll_interval: 15s
        "#;

        let a: A = serde_yaml::from_str(yaml_data).unwrap();

        assert_eq!(
            a.config,
            PollingConfig {
                initial_wait: Duration::from_secs(10),
                poll_interval: Duration::from_secs(15),
                max_wait: Duration::from_secs(600),
                error_wait: Duration::from_secs(10),
                max_errors: 5,
            }
        );
    }
}
