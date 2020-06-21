
use jack::{Client, ClientOptions, Error, MidiOut};

struct JackHandle {
    pub client: Client,
    pub ports: Vec<MidiOut>
}

impl JackHandle {
    fn new() -> Result<JackHandle, Error> {
        let (cl, status) = jack::Client::new(
            "noisefunge",
            ClientOptions::NO_START_SERVER)?;
        println!("Opened client {}, with status {:?}.",
                 cl.name(),
                 status);
        Ok(JackHandle { client: cl,
                        ports: Vec::new() })
    }
}
