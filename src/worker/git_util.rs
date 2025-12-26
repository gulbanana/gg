//! conceptually similar to jj_cli::git_util, but non-blocking

use crate::messages::{InputField, InputRequest, InputResponse, MultilineString, MutationResult};
use jj_lib::git::RemoteCallbacks;
use std::cell::RefCell;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
enum AuthRequirement {
    Password { url: String, username: String },
    UsernamePassword { url: String },
}

impl AuthRequirement {
    pub fn to_input_request(&self) -> InputRequest {
        match self {
            AuthRequirement::Password { url, username } => InputRequest {
                title: "Git Login".to_string(),
                detail: format!("Please enter a password for {} at {}", username, url),
                fields: vec![InputField {
                    label: "Password".to_string(),
                    choices: vec![],
                }],
            },
            AuthRequirement::UsernamePassword { url } => InputRequest {
                title: "Git Login".to_string(),
                detail: format!("Please enter a username and password for {}", url),
                fields: vec![
                    InputField {
                        label: "Username".to_string(),
                        choices: vec![],
                    },
                    InputField {
                        label: "Password".to_string(),
                        choices: vec![],
                    },
                ],
            },
        }
    }
}

pub struct AuthContext {
    input: Option<InputResponse>,
    // interior mutability: callbacks are single-threaded, but unpredictable
    requirements: RefCell<Vec<AuthRequirement>>,
}

impl AuthContext {
    pub fn new(input: Option<InputResponse>) -> Self {
        Self {
            input,
            requirements: RefCell::new(vec![]),
        }
    }

    // locally-owned callbacks, with explictly-unspecified lifetimes to satisfy HRTB
    pub fn with_callbacks<T>(&mut self, f: impl FnOnce(RemoteCallbacks) -> T) -> T {
        let mut callbacks = RemoteCallbacks::default();

        let mut get_ssh_keys = Self::get_ssh_keys;
        callbacks.get_ssh_keys = Some(&mut get_ssh_keys);

        let mut get_password = |url: &str, username: &str| self.get_password(url, username);
        callbacks.get_password = Some(&mut get_password);

        let mut get_username_password = |url: &str| self.get_username_password(url);
        callbacks.get_username_password = Some(&mut get_username_password);

        let result = f(callbacks);

        result
    }

    // simplistic, but it's the same as the version in jj_cli::git_util
    fn get_ssh_keys(_username: &str) -> Vec<PathBuf> {
        let mut paths = vec![];
        if let Some(home_dir) = dirs::home_dir() {
            let ssh_dir = Path::new(&home_dir).join(".ssh");
            for filename in ["id_ed25519_sk", "id_ed25519", "id_rsa"] {
                let key_path = ssh_dir.join(filename);
                if key_path.is_file() {
                    log::debug!("found ssh key {key_path:?}");
                    paths.push(key_path);
                }
            }
        }
        if paths.is_empty() {
            log::warn!("No SSH key found.");
        }
        paths
    }

    // return input if available, record requirement otherwise
    fn get_password(&self, url: &str, username: &str) -> Option<String> {
        if let Some(input) = self.input.clone()
            && let Some(password) = input.fields.get("Password")
        {
            Some(password.clone())
        } else {
            self.requirements
                .borrow_mut()
                .push(AuthRequirement::Password {
                    url: url.to_string(),
                    username: username.to_string(),
                });
            None
        }
    }

    // return inputs if available, record requirement otherwise
    fn get_username_password(&self, url: &str) -> Option<(String, String)> {
        if let Some(input) = self.input.clone()
            && let (Some(username), Some(password)) =
                (input.fields.get("Username"), input.fields.get("Password"))
        {
            Some((username.clone(), password.clone()))
        } else {
            self.requirements
                .borrow_mut()
                .push(AuthRequirement::UsernamePassword {
                    url: url.to_string(),
                });
            None
        }
    }

    // XXX currently supports only one requirement per mutation, which will fail with multiple unauthenticated remotes
    pub fn into_result(self, err: anyhow::Error) -> MutationResult {
        let mut requirements = self.requirements.into_inner();
        match requirements.len() {
            0 => MutationResult::InternalError {
                message: MultilineString::from(err.to_string().as_str()),
            },
            1 => MutationResult::InputRequired {
                request: requirements.remove(0).to_input_request(),
            },
            _ => panic!("multiple auth requirements not yet implemented"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use std::collections::HashMap;

    #[test]
    fn test_password_input_request() {
        let auth = AuthRequirement::Password {
            url: "https://github.com".to_string(),
            username: "user".to_string(),
        };

        let request = auth.to_input_request();

        assert_eq!(request.title, "Git Login");
        assert_eq!(request.fields.len(), 1);
        assert_eq!(request.fields[0].label, "Password");
    }

    #[test]
    fn test_username_password_input_request() {
        let auth = AuthRequirement::UsernamePassword {
            url: "https://gitlab.com".to_string(),
        };

        let request = auth.to_input_request();

        assert_eq!(request.title, "Git Login");
        assert_eq!(request.fields.len(), 2);
        assert_eq!(request.fields[0].label, "Username");
        assert_eq!(request.fields[1].label, "Password");
    }

    #[test]
    fn test_context_with_password() {
        let mut fields = HashMap::new();
        fields.insert("Password".to_string(), "secret".to_string());
        let input = Some(InputResponse { fields });
        let ctx = AuthContext::new(input);

        let result = ctx.get_password("https://github.com", "user");

        assert_eq!(result, Some("secret".to_string()));
        assert!(ctx.requirements.borrow().is_empty());
    }

    #[test]
    fn test_context_without_password() {
        let input = None;
        let ctx = AuthContext::new(input);

        let result = ctx.get_password("https://github.com", "user");

        assert_eq!(result, None);
        assert_eq!(ctx.requirements.borrow().len(), 1);
        assert_matches!(
            ctx.requirements.borrow()[0],
            AuthRequirement::Password { .. }
        );
    }

    #[test]
    fn test_context_with_username_password() {
        let mut fields = HashMap::new();
        fields.insert("Username".to_string(), "user".to_string());
        fields.insert("Password".to_string(), "secret".to_string());
        let input = Some(InputResponse { fields });
        let ctx = AuthContext::new(input);

        let result = ctx.get_username_password("https://github.com");

        assert_eq!(result, Some(("user".to_string(), "secret".to_string())));
    }

    #[test]
    fn test_context_without_username_password() {
        let input = None;
        let ctx = AuthContext::new(input);

        let result = ctx.get_username_password("https://github.com");

        assert_eq!(result, None);
        assert_eq!(ctx.requirements.borrow().len(), 1);
        assert_matches!(
            ctx.requirements.borrow()[0],
            AuthRequirement::UsernamePassword { .. }
        );
    }

    #[test]
    fn test_into_result_with_auth_required() {
        let input = None;
        let ctx = AuthContext::new(input);
        ctx.get_password("https://github.com", "user"); // records auth requirement

        let result = ctx.into_result(anyhow::anyhow!("some error"));

        assert_matches!(result, MutationResult::InputRequired { .. });
    }

    #[test]
    fn test_into_result_without_auth_required() {
        let input = None;
        let ctx = AuthContext::new(input);

        let result = ctx.into_result(anyhow::anyhow!("network error"));

        assert_matches!(result, MutationResult::InternalError { .. });
    }
}
