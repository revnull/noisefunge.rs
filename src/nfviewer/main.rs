
use noisefunge::api::*;
use noisefunge::config::*;
use clap::{Arg, App};

fn read_args() -> String {
    let matches = App::new("nfviewer")
                          .arg(Arg::with_name("HOST")
                               .help("Noisefunge server host")
                               .required(false)
                               .default_value("localhost"))
                          .arg(Arg::with_name("PORT")
                               .help("Noisefunge server port")
                               .required(false)
                               .default_value("1312"))
                          .get_matches();

    format!("http://{}:{}/", matches.value_of("HOST").unwrap(),
                             matches.value_of("PORT").unwrap())
}

fn main() {

    let baseuri = read_args();

    println!("baseuri: {}", baseuri);

}
