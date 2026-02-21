#![cfg(feature = "integration")]

mod common;

#[path = "integration/config_file_flag.rs"]
mod config_file_flag;
#[path = "integration/crash_recovery.rs"]
mod crash_recovery;
#[path = "integration/dir_discovery.rs"]
mod dir_discovery;
#[path = "integration/label_cleanup.rs"]
mod label_cleanup;
#[path = "integration/multi_instance.rs"]
mod multi_instance;
#[path = "integration/port_collision.rs"]
mod port_collision;
#[path = "integration/ps_all.rs"]
mod ps_all;
#[path = "integration/start_stop.rs"]
mod start_stop;
