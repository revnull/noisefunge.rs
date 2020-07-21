
use clap::{Arg, App};
use noisefunge::api::*;
use reqwest::blocking::Client;
use std::time::Duration;
use serde_json;

fn read_args() -> (Vec<u64>, String) {
    let matches = App::new("nfkill")
                          .arg(Arg::with_name("PID")
                               .help("PID of process to kill, in hex.")
                               .multiple(true)
                               .required_unless("ALL"))
                          .arg(Arg::with_name("ALL")
                               .short("a")
                               .long("--all")
                               .conflicts_with("PID")
                               .takes_value(false)
                               .required(false))
                          .arg(Arg::with_name("HOST")
                               .long("host")
                               .short("h")
                               .help("Noisefunge server host")
                               .required(false)
                               .env("NOISEFUNGE_HOST")
                               .default_value("localhost"))
                          .arg(Arg::with_name("PORT")
                               .long("port")
                               .short("p")
                               .help("Noisefunge server port")
                               .required(false)
                               .env("NOISEFUNGE_PORT")
                               .default_value("1312"))
                          .get_matches();

    let baseuri = format!("http://{}:{}/", matches.value_of("HOST").unwrap(),
                                           matches.value_of("PORT").unwrap());

    if matches.is_present("ALL") {
        return (Vec::new(), baseuri)
    }

    let mut pids = Vec::new();
    for pid in matches.values_of("PID").unwrap() {
        let parsed = u64::from_str_radix(pid, 16).expect(
            &format!("Failed to parse pid {}", pid));
        pids.push(parsed);
    }

    return (pids, baseuri)
}

fn main() {

    let (pids, baseuri) = read_args();

    let client = Client::builder().user_agent("nfkill")
                                  .build()
                                  .expect("Failed to build client.");

    let body = serde_json::to_string(&KillReq{ pids: pids }).unwrap();
    let path = format!("{}kill", baseuri);
    let request = client.post(&path)
                        .body(body)
                        .header("Content-Type", "application/json")
                        .timeout(Duration::from_secs(4))
                        .build()
                        .expect("Failed to build request");

    let response = client.execute(request);
    std::process::exit(match response {
        Ok(response) => {
            if response.status().is_success() {
                0
            } else {
                eprintln!("Error response: {:?}", response.status());
                1
            }
        }, 
        Err(err) => {
            eprintln!("Failed: {:?}", err);
            1
        }
    });

}
