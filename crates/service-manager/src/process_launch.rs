use std::{any::Any, collections::BTreeMap, sync::Arc};

use alan_agent_engine::{ProcessDescriptor, ProcessPackageReference};
use alan_kernel::{Credentials, LiveNamespace, Namespace};
use anyhow::Result;

/// Service-owned Process assembly inputs. Agent Execution Engine never receives
/// namespace, credential, descriptor-number, or retained authority ownership.
#[derive(Clone)]
pub struct ProcessLaunchContext {
    pub namespace: Namespace,
    pub descriptors: BTreeMap<String, ProcessDescriptor>,
    pub package_references: Vec<ProcessPackageReference>,
    pub credentials: Credentials,
    pub cwd: String,
    live_namespace: Option<LiveNamespace>,
    retained_authorities: Vec<Arc<dyn Any + Send + Sync>>,
}

impl std::fmt::Debug for ProcessLaunchContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProcessLaunchContext")
            .field("namespace", &self.namespace.describe())
            .field("descriptors", &self.descriptors)
            .field("package_references", &self.package_references)
            .field("credentials", &self.credentials)
            .field("cwd", &self.cwd)
            .field("live_namespace", &self.live_namespace.is_some())
            .field("retained_authorities", &self.retained_authorities.len())
            .finish()
    }
}

impl ProcessLaunchContext {
    pub fn new(
        namespace: Namespace,
        credentials: Credentials,
        cwd: impl Into<String>,
    ) -> Result<Self> {
        Ok(Self {
            namespace,
            descriptors: BTreeMap::new(),
            package_references: Vec::new(),
            credentials,
            cwd: normalize_namespace_path(&cwd.into())?,
            live_namespace: None,
            retained_authorities: Vec::new(),
        })
    }

    pub fn root() -> Self {
        Self::new(Namespace::new(), Credentials::user("root-agent"), "/")
            .expect("root Process launch context is valid")
    }

    pub fn with_descriptor(
        mut self,
        name: impl Into<String>,
        descriptor: ProcessDescriptor,
    ) -> Self {
        self.descriptors.insert(name.into(), descriptor);
        self
    }

    pub fn descriptor(&self, name: &str) -> Option<&ProcessDescriptor> {
        self.descriptors.get(name)
    }

    pub fn add_package_reference(&mut self, reference: ProcessPackageReference) {
        self.package_references.push(reference);
    }

    pub fn retain_authority<T>(&mut self, authority: Arc<T>)
    where
        T: Any + Send + Sync,
    {
        self.retained_authorities.push(authority);
    }

    pub fn child(&self) -> Self {
        Self {
            namespace: self.namespace_snapshot(),
            descriptors: self.descriptors.clone(),
            package_references: self.package_references.clone(),
            credentials: self.credentials.clone(),
            cwd: self.cwd.clone(),
            live_namespace: None,
            retained_authorities: self.retained_authorities.clone(),
        }
    }

    pub fn namespace_snapshot(&self) -> Namespace {
        self.live_namespace
            .as_ref()
            .map(LiveNamespace::snapshot)
            .unwrap_or_else(|| self.namespace.child())
    }

    pub fn rebound(&self, namespace: Namespace, credentials: Credentials) -> Self {
        Self {
            namespace,
            descriptors: self.descriptors.clone(),
            package_references: self.package_references.clone(),
            credentials,
            cwd: self.cwd.clone(),
            live_namespace: None,
            retained_authorities: self.retained_authorities.clone(),
        }
    }

    pub fn rebound_live(&self, namespace: LiveNamespace, credentials: Credentials) -> Self {
        Self {
            namespace: namespace.snapshot(),
            descriptors: self.descriptors.clone(),
            package_references: self.package_references.clone(),
            credentials,
            cwd: self.cwd.clone(),
            live_namespace: Some(namespace),
            retained_authorities: self.retained_authorities.clone(),
        }
    }
}

fn normalize_namespace_path(path: &str) -> Result<String> {
    let components = path
        .strip_prefix('/')
        .ok_or_else(|| anyhow::anyhow!("Alan OS path must be absolute: {path}"))?
        .split('/')
        .filter(|component| !component.is_empty())
        .map(|component| {
            anyhow::ensure!(
                component != "." && component != "..",
                "invalid Alan OS path: {path}"
            );
            Ok(component)
        })
        .collect::<Result<Vec<_>>>()?;
    if components.is_empty() {
        Ok("/".to_string())
    } else {
        Ok(format!("/{}", components.join("/")))
    }
}
