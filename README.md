# Noisefunge.rs

A reimplementation of noisefunge in rust. This is likely to become the
standard implementation of the language.

## Features
 * Built against JACK and should work on platforms supported by that API.
 * Provides midi input for receiving midi beat clock messages.
 * The server for handling requests is now built on HTTP and json.
 * Configuration file can be used to automatically send program select messages.
 * New semantics for defining custom opcodes.
 * Unicode support in the viewer so that all 256 byte values have a printable representation.
 * Built in arpeggiators.
