
use clap::{Arg, App};
use std::fs;
use noisefunge::api::*;
use reqwest::blocking::Client;
use std::time::Duration;

fn read_args() -> (String, String) {
    let matches = App::new("nfloader")
                          .arg(Arg::with_name("FILE")
                               .help("File containing noisefunge program.")
                               .required(true))
                          .arg(Arg::with_name("HOST")
                               .help("Noisefunge server host")
                               .required(false)
                               .env("NOISEFUNGE_HOST")
                               .default_value("localhost"))
                          .arg(Arg::with_name("PORT")
                               .help("Noisefunge server port")
                               .required(false)
                               .env("NOISEFUNGE_PORT")
                               .default_value("1312"))
                          .get_matches();

    let baseuri = format!("http://{}:{}/", matches.value_of("HOST").unwrap(),
                                           matches.value_of("PORT").unwrap());

    (String::from(matches.value_of("FILE").unwrap()), baseuri)
}

fn main() {

    let (filename, baseuri) = read_args();

    let err = format!("Failed to open {}", &filename);
    let prog = fs::read_to_string(&filename).expect(&err)
                                            .trim_end_matches('\n')
                                            .to_string();

    let client = Client::builder().user_agent("nfloader")
                                  .build()
                                  .expect("Failed to build client.");

    let body = serde_json::to_string(&NewProcessReq { name: Some(filename),
                                                      program: prog }).unwrap();
    let path = format!("{}process", baseuri);
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
                let resp: NewProcessResp = response.json().unwrap();
                println!("{:X}", resp.pid);
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
