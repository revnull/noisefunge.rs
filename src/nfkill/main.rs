
use clap::{Arg, App};
use std::fs;
use std::process;
use noisefunge::api::*;
use reqwest::blocking::Client;
use std::time::Duration;

fn read_args() -> (u64, String) {
    let matches = App::new("nfkill")
                          .arg(Arg::with_name("PID")
                               .help("PID of process to kill, in hex.")
                               .multiple(true)
                               .required(false))
                          .arg(Arg::with_name("ALL")
                               .conflicts_with("PID")
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

    let pid = u64::from_str_radix(matches.value_of("PID").unwrap(),
                                  16).expect("Failed to parse PID");
                     
    (pid, baseuri)
}

fn main() {

    let (pid, baseuri) = read_args();

    
/*
    let client = Client::builder().user_agent("nfloader")
                                  .build()
                                  .expect("Failed to build client.");

    let path = format!("{}process", baseuri);
    let request = client.post(&path)
                        .body(prog)
                        .header("Content-Type", "text/plain")
                        .timeout(Duration::from_secs(4))
                        .build()
                        .expect("Failed to build request");
    let response = client.execute(request);
    std::process::exit(match response {
        Ok(response) => {
            if response.status().is_success() {
                let resp: NewProcessResp = response.json().unwrap();
                println!("{}", resp.pid);
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
    */
}
