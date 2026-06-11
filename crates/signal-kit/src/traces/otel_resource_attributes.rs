use std::sync::OnceLock;

use opentelemetry::{KeyValue, Value};
use opentelemetry_sdk::Resource;
use opentelemetry_semantic_conventions::{
    attribute::{
        DEPLOYMENT_ENVIRONMENT_NAME, SERVICE_NAME, VCS_REF_HEAD_NAME, VCS_REF_HEAD_REVISION,
        VCS_REPOSITORY_URL_FULL,
    },
    resource::{
        CLOUD_PLATFORM, CLOUD_REGION, K8S_CONTAINER_NAME, K8S_DEPLOYMENT_NAME, K8S_NAMESPACE_NAME,
        K8S_NODE_NAME, K8S_POD_NAME, K8S_REPLICASET_NAME, SERVICE_INSTANCE_ID,
        TELEMETRY_SDK_LANGUAGE, TELEMETRY_SDK_NAME, TELEMETRY_SDK_VERSION,
    },
};
use ulid::Ulid;

// Static OnceLock instances for caching
static GIT_KEY_VALUES: OnceLock<Vec<KeyValue>> = OnceLock::new();
static CLOUD_KEY_VALUES: OnceLock<Vec<KeyValue>> = OnceLock::new();
static DEPLOYMENT_ENVIRONMENT: OnceLock<KeyValue> = OnceLock::new();

/// Returns OpenTelemetry key-value attributes for service identification.
///
/// This function creates a set of OpenTelemetry key-value pairs that identify
/// the service in telemetry data. It includes the service name (converted to
/// kebab-case) and a unique instance ID generated for this instance of the
/// service.
///
/// # Arguments
///
/// * `service_name` - The name of the service to be identified in telemetry
///
/// # Returns
///
/// A vector of KeyValue pairs for service identification
pub fn get_service_key_values(service_name: &str) -> Vec<KeyValue> {
    let service_name_attribute =
        KeyValue::new(SERVICE_NAME, heck::ToKebabCase::to_kebab_case(service_name));
    let service_instance_attribute = KeyValue::new(SERVICE_INSTANCE_ID, Ulid::new().to_string());
    vec![service_name_attribute, service_instance_attribute]
}

// Create a Resource that captures information about the entity for which
// telemetry is recorded.
pub(crate) fn resource(service_name: &str) -> Resource {
    build_resource(service_name, [])
}

/// Create a Resource that captures information about the entity for which
/// telemetry is recorded, with additional custom attributes.
///
/// Additional attributes will be merged at the end, allowing them to override
/// any default attributes if there are key conflicts.
///
/// # Arguments
///
/// * `service_name` - The name of the service
/// * `additional_attributes` - Additional resource attributes to include
///
/// # Returns
///
/// A Resource with the combined attributes
pub fn build_resource(
    service_name: impl Into<String>,
    additional_attributes: impl IntoIterator<Item = KeyValue>,
) -> Resource {
    let service_name = service_name.into();
    let git_attributes = get_git_key_values();
    let cloud_attributes = get_cloud_key_values();
    let k8s_attributes = get_k8s_key_values(&service_name);
    let deployment_environment = get_deployment_environment();
    let service_attributes = get_service_key_values(&service_name);

    let mut attributes: Vec<KeyValue> = git_attributes
        .into_iter()
        .chain(cloud_attributes)
        .chain(k8s_attributes)
        .chain(std::iter::once(deployment_environment))
        .chain(service_attributes)
        .chain(vec![
            KeyValue::new(SERVICE_NAME, service_name.clone()),
            KeyValue::new(TELEMETRY_SDK_NAME, "opentelemetry".to_string()),
            KeyValue::new(TELEMETRY_SDK_LANGUAGE, "rust".to_string()),
            KeyValue::new(TELEMETRY_SDK_VERSION, env!("CARGO_PKG_VERSION").to_string()),
        ])
        .collect();

    attributes.extend(parse_otel_resource_attributes());
    attributes.extend(additional_attributes);

    attributes.retain(|attr| attr.key.as_str() != SERVICE_NAME);
    attributes.push(KeyValue::new(SERVICE_NAME, service_name));

    Resource::builder().with_attributes(attributes).build()
}

const UNKNOWN_VALUE: &str = "unknown";
/// Get the git key values
/// These are manually supplied via the environment variables from the CD
/// pipeline
fn get_git_key_values() -> Vec<KeyValue> {
    GIT_KEY_VALUES
        .get_or_init(|| {
            const BUILD_NUMBER: &str = "build.number";

            // Get the git commit, branch and repo_url from the environment variables at
            // compile time
            let git_commit = option_env!("GIT_COMMIT")
                .unwrap_or(UNKNOWN_VALUE)
                .to_string();
            let git_branch = option_env!("GIT_BRANCH")
                .unwrap_or(UNKNOWN_VALUE)
                .to_string();
            let git_repo_url = option_env!("GIT_REPOSITORY_URL")
                .unwrap_or(UNKNOWN_VALUE)
                .to_string();

            // Get the non-standard build number and pipeline id from the
            // environment variables at compile time (embedded by build.rs)
            let build_number = option_env!("BUILD_NUMBER")
                .unwrap_or(UNKNOWN_VALUE)
                .to_string();

            vec![
                KeyValue::new(VCS_REF_HEAD_REVISION, git_commit),
                KeyValue::new(VCS_REF_HEAD_NAME, git_branch),
                KeyValue::new(VCS_REPOSITORY_URL_FULL, git_repo_url),
                KeyValue::new(BUILD_NUMBER, build_number),
            ]
        })
        .clone()
}

/// Get the cloud key values
/// These are manually supplied via the environment variables from the CD
/// pipeline
fn get_cloud_key_values() -> Vec<KeyValue> {
    CLOUD_KEY_VALUES
        .get_or_init(|| {
            let mut attributes = Vec::new();

            let cloud_datacenter = std::env::var("CLOUD_DATACENTER").unwrap_or_default();
            if !cloud_datacenter.is_empty() {
                attributes.push(KeyValue::new(CLOUD_REGION, cloud_datacenter));
            }

            if let Ok(cloud_platform) = std::env::var("CLOUD_PLATFORM") {
                attributes.push(KeyValue::new(CLOUD_PLATFORM, cloud_platform));
            }

            attributes
        })
        .clone()
}

/// Get the deployment environment key value
/// This is manually supplied via the environment variables from the CD pipeline
fn get_deployment_environment() -> KeyValue {
    DEPLOYMENT_ENVIRONMENT
        .get_or_init(|| {
            const ENVIRONMENT_ENV_VAR: &str = "ENVIRONMENT";
            // Retrieve environment name
            let environment = match std::env::var(ENVIRONMENT_ENV_VAR) {
                Ok(environment) => environment,
                Err(_) => "unknown".to_string(),
            };
            KeyValue::new(DEPLOYMENT_ENVIRONMENT_NAME, environment)
        })
        .clone()
}

/// Get the k8s key values
/// There are some non-semantic convention attributes which we add anyway
/// These are:
///     - k8s.pod.ip
///     - k8s.host.ip
///     - k8s.service.account
/// The rest are semantic convention attributes
fn get_k8s_key_values(service_name: &str) -> Vec<KeyValue> {
    /// These are non-semantic convention attributes which we add anyway
    const K8S_POD_IP: &str = "k8s.pod.ip";
    const K8S_HOST_IP: &str = "k8s.host.ip";
    const K8S_SERVICE_ACCOUNT: &str = "k8s.service.account";

    let namespace = std::env::var("KUBERNETES_NAMESPACE").unwrap_or_default();
    let pod_name = std::env::var("KUBERNETES_POD_NAME").unwrap_or_default();
    let node_name = std::env::var("KUBERNETES_NODE_NAME").unwrap_or_default();
    let pod_ip = std::env::var("KUBERNETES_POD_IP").unwrap_or_default();
    let host_ip = std::env::var("KUBERNETES_HOST_IP").unwrap_or_default();
    let service_account = std::env::var("KUBERNETES_SERVICE_ACCOUNT").unwrap_or_default();
    let container_name =
        std::env::var("KUBERNETES_CONTAINER_NAME").unwrap_or(service_name.to_string());
    let replicaset_name = extract_replicaset_name(&pod_name);
    let deployment_name = extract_deployment_name(&pod_name);
    vec![
        KeyValue::new(K8S_NAMESPACE_NAME, namespace),
        KeyValue::new(K8S_POD_NAME, pod_name),
        KeyValue::new(K8S_NODE_NAME, node_name),
        KeyValue::new(K8S_POD_IP, pod_ip),
        KeyValue::new(K8S_HOST_IP, host_ip),
        KeyValue::new(K8S_SERVICE_ACCOUNT, service_account),
        KeyValue::new(K8S_CONTAINER_NAME, container_name),
        KeyValue::new(K8S_REPLICASET_NAME, replicaset_name),
        KeyValue::new(K8S_DEPLOYMENT_NAME, deployment_name),
    ]
}

// Helper function for extracting ReplicaSet name
fn extract_replicaset_name(pod_name: &str) -> String {
    let parts: Vec<&str> = pod_name.split('-').collect();
    if parts.len() >= 2 {
        parts[..parts.len() - 1].join("-")
    } else {
        "".to_string()
    }
}

/// Helper function for extracting deployment name
fn extract_deployment_name(pod_name: &str) -> String {
    let parts: Vec<&str> = pod_name.split('-').collect();
    if parts.len() >= 3 {
        // Take all parts except the last two
        parts[..parts.len() - 2].join("-")
    } else {
        "".to_string()
    }
}

// Helper function to redact passwords from URLs.
// Replaces 'scheme://user:password@host' with 'scheme://user:redacted@host'.
#[allow(dead_code)]
fn redact_url_password(url_str: &str) -> String {
    if let Some(scheme_separator_idx) = url_str.find("://") {
        // Search for '@' after "://"
        if let Some(at_separator_idx_rel) = url_str[scheme_separator_idx + 3..].find('@') {
            let at_separator_idx_abs = scheme_separator_idx + 3 + at_separator_idx_rel;
            let user_info_part = &url_str[scheme_separator_idx + 3..at_separator_idx_abs];

            if let Some(colon_idx_in_userinfo) = user_info_part.find(':') {
                // Check if there's content after the colon (i.e., a password to redact)
                if colon_idx_in_userinfo + 1 < user_info_part.len() {
                    // Construct the redacted URL: "scheme://user" + ":redacted" + "@host..."
                    let up_to_colon_in_auth =
                        &url_str[..scheme_separator_idx + 3 + colon_idx_in_userinfo];
                    let after_at_symbol = &url_str[at_separator_idx_abs..];
                    return format!("{up_to_colon_in_auth}:redacted{after_at_symbol}");
                }
            }
        }
    }
    url_str.to_string() // No redaction needed or pattern not matched
}

/// Helper function to determine if a resource attribute value should be
/// considered "unset" (i.e., not worth printing).
///
/// An "unset" value is defined as:
/// - The literal string "unknown"
/// - An empty string ""
#[allow(dead_code)]
fn is_unset_value(value: &str) -> bool {
    value.is_empty() || value == UNKNOWN_VALUE
}

fn parse_otel_resource_attributes() -> Vec<KeyValue> {
    let Ok(raw) = std::env::var("OTEL_RESOURCE_ATTRIBUTES") else {
        return Vec::new();
    };

    raw.split(',')
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            let key = key.trim();
            let value = value.trim();
            if key.is_empty() {
                return None;
            }
            Some(KeyValue::new(
                key.to_owned(),
                Value::String(value.to_owned().into()),
            ))
        })
        .collect()
}

/// Returns printable OpenTelemetry resource attributes with secrets redacted.
#[allow(dead_code)]
pub(crate) fn printable_otel_resource(resource: &Resource) -> Vec<(String, String)> {
    let mut attributes_to_print: Vec<(String, String)> = Vec::new();

    for (key, value) in resource.iter() {
        let key_str = key.as_str().to_string();
        let mut value_display_str = match value {
            opentelemetry::Value::String(s) => s.to_string(),
            opentelemetry::Value::Bool(b) => b.to_string(),
            opentelemetry::Value::I64(i) => i.to_string(),
            opentelemetry::Value::F64(f) => f.to_string(),
            opentelemetry::Value::Array(arr) => format!("{arr:?}"),
            _ => "<unsupported value type>".to_string(),
        };

        // Check if the current key is for the VCS repository URL and redact if
        // necessary
        if key.as_str() == VCS_REPOSITORY_URL_FULL {
            value_display_str = redact_url_password(&value_display_str);
        }

        // Skip attributes with unset values
        if !is_unset_value(&value_display_str) {
            attributes_to_print.push((key_str, value_display_str));
        }
    }

    attributes_to_print.sort_by(|a, b| a.0.cmp(&b.0));
    attributes_to_print
}
