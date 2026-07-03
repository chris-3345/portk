use clap::Parser;
use regex::Regex;
use std::collections::HashSet;
use std::io::{self, Write};
use std::process::Command;
use sysinfo::{Pid, System};

#[derive(Parser, Debug)]
struct Args {
    ports: Vec<u16>,
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn run_unix(args: &Args) -> Vec<String> {
    let mut pids = HashSet::new();

    for port in &args.ports {
        let port_arg = format!("-iTCP:{}", port);

        let output = Command::new("lsof")
            .args(["-t", &port_arg, "-sTCP:LISTEN"])
            .output()
            .unwrap_or_else(|err| {
                panic!("Failed to execute lsof for port {}: {}", port, err);
            });

        let stdout = String::from_utf8_lossy(&output.stdout);

        for line in stdout.lines() {
            let pid = line.trim();

            if !pid.is_empty() && pid.chars().all(|c| c.is_ascii_digit()) {
                pids.insert(pid.to_string());
            }
        }
    }

    pids.into_iter().collect()
}

#[cfg(target_os = "windows")]
fn run_windows(args: &Args) -> Vec<String> {
    let mut pids = HashSet::new();

    let output = Command::new("netstat")
        .arg("-ano")
        .output()
        .expect("Failed to execute netstat");

    let stdout = String::from_utf8_lossy(&output.stdout);

    for &port in &args.ports {
        let re_pattern = format!(r":{}\s+.*\s+(\d+)$", port);
        let re = Regex::new(&re_pattern).unwrap();

        for line in stdout.lines() {
            if let Some(caps) = re.captures(line) {
                if let Some(pid) = caps.get(1) {
                    pids.insert(pid.as_str().to_string());
                }
            }
        }
    }

    pids.into_iter().collect()
}

fn kill_pids(pids: &[String]) {
    let mut sys = System::new(); // init system interface
    let mut process_names: Vec<String> = Vec::new();

    // Get process names for a confirmation prompt
    for pid_str in pids {
        if let Ok(pid_num) = pid_str.parse::<usize>() {
            let pid = Pid::from(pid_num);

            sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true); // refresh PID info

            if let Some(proc) = sys.process(pid) {
                process_names.push(proc.name().to_string_lossy().into_owned());
            } else {
                process_names.push(format!("Unknown Process"));
            }
        } else {
            process_names.push(format!("Invalid PID"));
        }
    }

    if pids.is_empty() {
        println!("No active processes found on provided ports!");
        return;
    }

    if process_names
        .iter()
        .any(|name| name.to_lowercase().contains("ollama"))
    {
        println!("Ollama was detected in your list of processes to kill.");
        println!("Killing Ollama will NOT work because it will start the server again.");
        println!("Will try to kill Ollama anyway, but it is probably going to fail.\n");
    }

    let mut confirmation_prompt = String::new();
    print!(
        "Are you sure you want to kill these processes: {:?}? (y/N): ",
        process_names
    );

    io::stdout().flush().expect("Failed to flush stdout");

    io::stdin()
        .read_line(&mut confirmation_prompt)
        .expect("Failed to read input");

    if confirmation_prompt.trim().to_lowercase() == "y" {
        // Start killing the PIDs
        println!("Killing processes!");
        for pid_str in pids {
            if let Ok(pid_num) = pid_str.parse::<usize>() {
                let pid = Pid::from(pid_num);
                // refresh process AGAIN to make sure it didn't close
                sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);

                if let Some(proc) = sys.process(pid) {
                    if proc.kill() {
                        println!("Killed PID {} ({})", pid_str, proc.name().to_string_lossy());
                    } else {
                        println!("Failed to kill PID {} (Permission denied?)", pid_str);
                    }
                } else {
                    println!("PID {} closed on its own...", pid_str);
                }
            }
        }
    } else {
        println!("Canceled.");
    }
}

fn main() {
    let args = Args::parse();

    if args.ports.is_empty() {
        println!("No ports provided! Usage: portk [PORTS]");
        return;
    }

    #[cfg(target_os = "windows")]
    let proc_ids = run_windows(&args);

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let proc_ids = run_unix(&args);

    kill_pids(&proc_ids);
}
