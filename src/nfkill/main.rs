/*
    Noisefunge Copyright (C) 2021 Rev. Johnny Healey <rev.null@gmail.com>

    This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

use clap::{Arg, App};
use noisefunge::api::*;
use reqwest::blocking::Client;
use std::time::Duration;
use serde_json;

fn read_args() -> (KillReq, String) {
    let matches = App::new("nfkill")
                          .arg(Arg::with_name("PID_OR_NAME")
                               .help("PID OR NAME of process to kill, in hex.")
                               .multiple(true)
                               .required_unless("ALL"))
                          .arg(Arg::with_name("ALL")
                               .short("a")
                               .long("--all")
                               .conflicts_with("PID_OR_NAME")
                               .takes_value(false)
                               .required(false))
                          .arg(Arg::with_name("NAME")
                               .short("n")
                               .long("--name")
                               .conflicts_with("ALL")
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
        return (KillReq::All, baseuri)
    }

    if matches.is_present("NAME") {
        let mut names = Vec::new();
        for name in matches.values_of("PID_OR_NAME").unwrap() {
            names.push(name.to_string());
        }
        return (KillReq::Names(names), baseuri);
    }

    let mut pids = Vec::new();
    for pid in matches.values_of("PID_OR_NAME").unwrap() {
        let parsed = u64::from_str_radix(pid, 16).expect(
            &format!("Failed to parse pid {}", pid));
        pids.push(parsed);
    }

    return (KillReq::Pids(pids), baseuri)
}

fn main() {

    let (req, baseuri) = read_args();

    let client = Client::builder().user_agent("nfkill")
                                  .build()
                                  .expect("Failed to build client.");

    let body = serde_json::to_string(&req).unwrap();
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
