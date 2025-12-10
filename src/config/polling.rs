use duration_str::deserialize_duration;
use std::time::Duration;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct PollingConfig {
    #[serde(
        deserialize_with = "deserialize_duration",
        default = "default_initial_wait"
    )]
    pub(crate) initial_wait: Duration,

    #[serde(
        deserialize_with = "deserialize_duration",
        default = "default_poll_interval"
    )]
    pub(crate) poll_interval: Duration,

    #[serde(
        deserialize_with = "deserialize_duration",
        default = "default_max_wait"
    )]
    pub(crate) max_wait: Duration,

    #[serde(
        deserialize_with = "deserialize_duration",
        default = "default_error_wait"
    )]
    pub(crate) error_wait: Duration,

    #[serde(default = "default_max_errors")]
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

const fn default_initial_wait() -> Duration {
    Duration::from_secs(10)
}
const fn default_poll_interval() -> Duration {
    Duration::from_secs(5)
}
const fn default_max_wait() -> Duration {
    Duration::from_secs(600)
}
const fn default_error_wait() -> Duration {
    Duration::from_secs(10)
}
const fn default_max_errors() -> u64 {
    5
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

    #[test]
    fn test_polling_config_deserialize_omitted() {
        let yaml_data = r#"{}"#;

        let a: A = serde_yaml::from_str(yaml_data).unwrap();

        assert_eq!(
            a.config,
            PollingConfig {
                initial_wait: Duration::from_secs(10),
                poll_interval: Duration::from_secs(5),
                max_wait: Duration::from_secs(600),
                error_wait: Duration::from_secs(10),
                max_errors: 5,
            }
        );
    }
}
