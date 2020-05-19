use clap::{App, Arg};
use std::path::{Path, PathBuf};
use std::vec::Vec;
use tempfile;

use crate::Result;

pub struct AppInfo {
    bin_path: PathBuf,
}

impl AppInfo {
    fn new(path: &str) -> AppInfo {
        AppInfo {
            bin_path: PathBuf::from(path),
        }
    }

    pub fn bin_path(&self) -> &Path {
        self.bin_path.as_path()
    }
}

pub struct TestClient {
    runtime_path: tempfile::TempDir,
    apps: Vec<AppInfo>,
}

impl TestClient {
    pub fn from_cmd_line_args() -> Result<TestClient> {
        let arg_match = App::new("The Tock kernel")
            .arg(
                Arg::with_name("apps")
                    .short("a")
                    .long("apps")
                    .help("A comma seperated list of path names to app binaries")
                    .takes_value(true)
                    .multiple(true)
                    .use_delimiter(true)
                    .required(false),
            )
            .get_matches();

        let runtime_path = tempfile::tempdir()?;

        let apps: Vec<AppInfo> = match arg_match.values_of("apps") {
            Some(app_list) => app_list.map(|app| AppInfo::new(app)).collect(),
            None => Vec::default(),
        };

        Ok(TestClient {
            runtime_path: runtime_path,
            apps: apps,
        })
    }

    pub fn irq_path(&self) -> PathBuf {
        self.runtime_path.path().join(Path::new("ext_irq"))
    }

    pub fn syscall_rx_path(&self) -> PathBuf {
        self.runtime_path.path().join(Path::new("kernel_rx"))
    }

    pub fn syscall_tx_path(&self) -> PathBuf {
        self.runtime_path.path().join(Path::new("kernel_tx"))
    }

    pub fn apps(&self) -> &Vec<AppInfo> {
        &self.apps
    }
}
