#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactState {
    Available,
    Downloading,
    Downloaded,
    Verifying,
    Verified,
    Installing,
    Installed,
    Loading,
    Ready,
    InUse,
    Unloading,
    Deleting,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleError {
    InvalidTransition {
        from: ArtifactState,
        to: ArtifactState,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelLifecycle {
    state: ArtifactState,
}

impl Default for ModelLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelLifecycle {
    pub fn new() -> Self {
        Self {
            state: ArtifactState::Available,
        }
    }

    pub fn state(&self) -> ArtifactState {
        self.state
    }

    pub fn transition(&mut self, to: ArtifactState) -> Result<(), LifecycleError> {
        let allowed = matches!(
            (self.state, to),
            (ArtifactState::Available, ArtifactState::Downloading)
                | (ArtifactState::Downloading, ArtifactState::Downloaded)
                | (ArtifactState::Downloading, ArtifactState::Failed)
                | (ArtifactState::Downloading, ArtifactState::Cancelled)
                | (ArtifactState::Downloaded, ArtifactState::Verifying)
                | (ArtifactState::Downloaded, ArtifactState::Deleting)
                | (ArtifactState::Verifying, ArtifactState::Verified)
                | (ArtifactState::Verifying, ArtifactState::Failed)
                | (ArtifactState::Verifying, ArtifactState::Cancelled)
                | (ArtifactState::Verified, ArtifactState::Installing)
                | (ArtifactState::Verified, ArtifactState::Deleting)
                | (ArtifactState::Installing, ArtifactState::Installed)
                | (ArtifactState::Installing, ArtifactState::Failed)
                | (ArtifactState::Installing, ArtifactState::Cancelled)
                | (ArtifactState::Installed, ArtifactState::Loading)
                | (ArtifactState::Installed, ArtifactState::Deleting)
                | (ArtifactState::Loading, ArtifactState::Ready)
                | (ArtifactState::Loading, ArtifactState::Installed)
                | (ArtifactState::Loading, ArtifactState::Failed)
                | (ArtifactState::Loading, ArtifactState::Cancelled)
                | (ArtifactState::Ready, ArtifactState::InUse)
                | (ArtifactState::Ready, ArtifactState::Unloading)
                | (ArtifactState::InUse, ArtifactState::Unloading)
                | (ArtifactState::Unloading, ArtifactState::Installed)
                | (ArtifactState::Unloading, ArtifactState::Failed)
                | (ArtifactState::Downloaded, ArtifactState::Failed)
                | (ArtifactState::Deleting, ArtifactState::Available)
                | (ArtifactState::Deleting, ArtifactState::Failed)
                | (ArtifactState::Failed, ArtifactState::Downloading)
                | (ArtifactState::Failed, ArtifactState::Deleting)
                | (ArtifactState::Cancelled, ArtifactState::Downloading)
                | (ArtifactState::Cancelled, ArtifactState::Deleting)
        );

        if !allowed {
            return Err(LifecycleError::InvalidTransition {
                from: self.state,
                to,
            });
        }
        self.state = to;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_download_can_be_retried() {
        let mut lifecycle = ModelLifecycle::new();
        lifecycle.transition(ArtifactState::Downloading).unwrap();
        lifecycle.transition(ArtifactState::Failed).unwrap();
        lifecycle.transition(ArtifactState::Downloading).unwrap();

        assert_eq!(lifecycle.state(), ArtifactState::Downloading);
    }

    #[test]
    fn cannot_delete_while_loaded_or_in_use() {
        let mut lifecycle = ModelLifecycle::new();
        for state in [
            ArtifactState::Downloading,
            ArtifactState::Downloaded,
            ArtifactState::Verifying,
            ArtifactState::Verified,
            ArtifactState::Installing,
            ArtifactState::Installed,
            ArtifactState::Loading,
            ArtifactState::Ready,
            ArtifactState::InUse,
        ] {
            lifecycle.transition(state).unwrap();
        }

        assert_eq!(
            lifecycle.transition(ArtifactState::Deleting),
            Err(LifecycleError::InvalidTransition {
                from: ArtifactState::InUse,
                to: ArtifactState::Deleting,
            })
        );
    }
}
