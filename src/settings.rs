use num_cpus;
use std::{env, fmt, io, net, path, time};

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:9412";
const DEFAULT_SCRIPT_DEFINITION_EXPIRATION_TIME: &str = "86400000";
const DEFAULT_SCRIPT_EXECUTION_COMPLETION_TIME: &str = "10000";
const DEFAULT_SCRIPT_EXECUTION_THREAD_POOL_SIZE: &str = "0";

/// Represents the settings for the program. These are sourced
/// strictly from environment variables.
pub(crate) struct Settings {
    pub(crate) bind_addr: net::SocketAddr,
    pub(crate) script_definition_expiration_time: time::Duration,
    pub(crate) script_execution_completion_time: time::Duration,
    pub(crate) script_execution_thread_pool_size: usize,
    pub(crate) tls_bind_addr: Option<net::SocketAddr>,
    pub(crate) tls_public_certificate_path: Option<path::PathBuf>,
    pub(crate) tls_private_key_path: Option<path::PathBuf>,
}

fn to_io_error<T, E: fmt::Display>(result: Result<T, E>) -> io::Result<T> {
    result.map_err(|e| io::Error::new(io::ErrorKind::Other, format!("{}", e)))
}

impl Settings {
    pub(crate) fn new(
        env_jsaas_bind_addr: &str,
        env_jsaas_script_definition_expiration_time: &str,
        env_jsaas_script_execution_thread_pool_size: &str,
        env_jsaas_script_execution_completion_time: &str,
        env_jsaas_tls_bind_addr: &str,
        env_jsaas_tls_public_certificate_path: &str,
        env_jsaas_tls_private_key_path: &str,
    ) -> io::Result<Settings> {
        let bind_addr =
            env::var(env_jsaas_bind_addr).unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_string());
        let script_definition_expiration_time =
            env::var(env_jsaas_script_definition_expiration_time)
                .unwrap_or_else(|_| DEFAULT_SCRIPT_DEFINITION_EXPIRATION_TIME.to_string());
        let script_execution_completion_time = env::var(env_jsaas_script_execution_completion_time)
            .unwrap_or_else(|_| DEFAULT_SCRIPT_EXECUTION_COMPLETION_TIME.to_string());
        let script_execution_thread_pool_size =
            env::var(env_jsaas_script_execution_thread_pool_size)
                .unwrap_or_else(|_| DEFAULT_SCRIPT_EXECUTION_THREAD_POOL_SIZE.to_string());

        let bind_addr = to_io_error(bind_addr.parse::<net::SocketAddr>())?;
        let script_definition_expiration_time_ms =
            to_io_error(script_definition_expiration_time.parse::<u64>())?;
        let script_execution_thread_pool_size =
            to_io_error(script_execution_thread_pool_size.parse::<usize>())?;
        let script_execution_completion_time_ms =
            to_io_error(script_execution_completion_time.parse::<u64>())?;

        let script_execution_thread_pool_size = if script_execution_thread_pool_size == 0 {
            num_cpus::get()
        } else {
            script_execution_thread_pool_size
        };

        let tls_bind_addr = match env::var(env_jsaas_tls_bind_addr).ok() {
            Some(a) => Some(to_io_error(a.parse::<net::SocketAddr>())?),
            None => None,
        };

        let tls_public_certificate_path = env::var(env_jsaas_tls_public_certificate_path)
            .ok()
            .filter(|p| !p.is_empty())
            .map(|p| path::Path::new(&p).to_path_buf());

        let tls_private_key_path = env::var(env_jsaas_tls_private_key_path)
            .ok()
            .filter(|p| !p.is_empty())
            .map(|p| path::Path::new(&p).to_path_buf());

        Ok(Settings {
            bind_addr,
            script_definition_expiration_time: time::Duration::from_millis(
                script_definition_expiration_time_ms,
            ),
            script_execution_completion_time: time::Duration::from_millis(
                script_execution_completion_time_ms,
            ),
            script_execution_thread_pool_size,
            tls_bind_addr,
            tls_public_certificate_path,
            tls_private_key_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, time};

    #[test]
    fn test_settings_default() {
        let settings = Settings::new(
            "JSAAS_TEST_1_BIND_ADDR",
            "JSAAS_TEST_1_SCRIPT_DEFINITION_EXPIRATION_TIME",
            "JSAAS_TEST_1_SCRIPT_EXECUTION_THREAD_POOL_SIZE",
            "JSAAS_TEST_1_SCRIPT_EXECUTION_COMPLETION_TIME",
            "JSAAS_TEST_1_TLS_BIND_ADDR",
            "JSAAS_TEST_1_TLS_PUBLIC_CERTIFICATE_PATH",
            "JSAAS_TEST_1_TLS_PRIVATE_KEY_PATH",
        )
        .unwrap();

        assert_eq!(
            settings.bind_addr,
            "127.0.0.1:9412".parse::<net::SocketAddr>().unwrap()
        );
        assert_eq!(
            settings.script_definition_expiration_time,
            time::Duration::from_secs(86400)
        );
        assert_eq!(
            settings.script_execution_completion_time,
            time::Duration::from_secs(10)
        );
        assert!(settings.script_execution_thread_pool_size > 0);

        assert_eq!(settings.tls_bind_addr, None);

        assert_eq!(settings.tls_public_certificate_path, None);

        assert_eq!(settings.tls_private_key_path, None);
    }

    #[test]
    fn test_settings_env_vars_valid() {
        env::set_var("JSAAS_TEST_2_BIND_ADDR", "127.0.0.2:1234");
        env::set_var("JSAAS_TEST_2_SCRIPT_DEFINITION_EXPIRATION_TIME", "5000");
        env::set_var("JSAAS_TEST_2_SCRIPT_EXECUTION_THREAD_POOL_SIZE", "7");
        env::set_var("JSAAS_TEST_2_SCRIPT_EXECUTION_COMPLETION_TIME", "1000");
        env::set_var("JSAAS_TEST_2_TLS_BIND_ADDR", "127.0.0.3:1235");
        env::set_var("JSAAS_TEST_2_TLS_PUBLIC_CERTIFICATE_PATH", "/root/pub.pem");
        env::set_var("JSAAS_TEST_2_TLS_PRIVATE_KEY_PATH", "/root/priv.pem");

        let settings = Settings::new(
            "JSAAS_TEST_2_BIND_ADDR",
            "JSAAS_TEST_2_SCRIPT_DEFINITION_EXPIRATION_TIME",
            "JSAAS_TEST_2_SCRIPT_EXECUTION_THREAD_POOL_SIZE",
            "JSAAS_TEST_2_SCRIPT_EXECUTION_COMPLETION_TIME",
            "JSAAS_TEST_2_TLS_BIND_ADDR",
            "JSAAS_TEST_2_TLS_PUBLIC_CERTIFICATE_PATH",
            "JSAAS_TEST_2_TLS_PRIVATE_KEY_PATH",
        )
        .unwrap();

        assert_eq!(
            settings.bind_addr,
            "127.0.0.2:1234".parse::<net::SocketAddr>().unwrap()
        );
        assert_eq!(
            settings.script_definition_expiration_time,
            time::Duration::from_secs(5)
        );
        assert_eq!(
            settings.script_execution_completion_time,
            time::Duration::from_secs(1)
        );
        assert_eq!(settings.script_execution_thread_pool_size, 7);

        assert_eq!(
            settings.tls_bind_addr,
            Some("127.0.0.3:1235".parse::<net::SocketAddr>().unwrap())
        );

        assert_eq!(
            settings.tls_public_certificate_path,
            Some(path::Path::new("/root/pub.pem").to_path_buf())
        );

        assert_eq!(
            settings.tls_private_key_path,
            Some(path::Path::new("/root/priv.pem").to_path_buf())
        );
    }

    #[test]
    fn test_settings_env_vars_invalid() {
        env::set_var("JSAAS_TEST_3_BIND_ADDR", "*@!($!");
        env::set_var("JSAAS_TEST_3_SCRIPT_DEFINITION_EXPIRATION_TIME", "");
        env::set_var("JSAAS_TEST_3_SCRIPT_EXECUTION_THREAD_POOL_SIZE", "");
        env::set_var("JSAAS_TEST_3_SCRIPT_EXECUTION_COMPLETION_TIME", "");

        assert!(Settings::new(
            "JSAAS_TEST_3_BIND_ADDR",
            "JSAAS_TEST_3_SCRIPT_DEFINITION_EXPIRATION_TIME",
            "JSAAS_TEST_3_SCRIPT_EXECUTION_THREAD_POOL_SIZE",
            "JSAAS_TEST_3_SCRIPT_EXECUTION_COMPLETION_TIME",
            "JSAAS_TEST_3_TLS_BIND_ADDR",
            "JSAAS_TEST_3_TLS_PUBLIC_CERTIFICATE_PATH",
            "JSAAS_TEST_3_TLS_PRIVATE_KEY_PATH"
        )
        .is_err());
    }
}
