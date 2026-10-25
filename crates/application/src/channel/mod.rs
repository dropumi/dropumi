// SPDX-License-Identifier: MPL-2.0
//! The secure channel: session establishment and encrypted record I/O.

mod establish;
mod io;

pub use establish::{
    EstablishedSession, InitiateParams, PeerInfo, RespondParams, ResponderDecision, SOFTWARE,
    initiate, respond,
};
pub use io::{RecordReader, RecordWriter, RecvError};
