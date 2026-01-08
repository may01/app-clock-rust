/*****************************************************************************
 *   Ledger App Boilerplate Rust.
 *   (c) 2023 Ledger SAS.
 *
 *  Licensed under the Apache License, Version 2.0 (the "License");
 *  you may not use this file except in compliance with the License.
 *  You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 *  Unless required by applicable law or agreed to in writing, software
 *  distributed under the License is distributed on an "AS IS" BASIS,
 *  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 *  See the License for the specific language governing permissions and
 *  limitations under the License.
 *****************************************************************************/

#![no_std]
#![no_main]

mod utils;
mod app_ui {
    pub mod address;
    pub mod menu;
    pub mod sign;
    //pub mod nbgl_clock;
}
mod handlers {
    pub mod get_public_key;
    pub mod get_version;
    pub mod sign_tx;
}

mod settings;

use app_ui::menu::ui_menu_main;
//use app_ui::nbgl_clock::NbglClock;
use handlers::{
    get_public_key::handler_get_public_key,
    get_version::handler_get_version,
    sign_tx::{handler_sign_tx, TxContext},
};
use ledger_device_sdk::{
    io::{ApduHeader, Comm, Reply, StatusWords},
    nbgl::init_comm,
};

ledger_device_sdk::set_panic!(ledger_device_sdk::exiting_panic);

// Required for using String, Vec, format!...
extern crate alloc;
use alloc::format;

use ledger_device_sdk::nbgl::{NbglReviewStatus, StatusType};
use ledger_device_sdk::nbgl::nbgl_clock::NbglClock;
use ledger_device_sdk::nbgl::NbglGlyph;
use ledger_device_sdk::include_gif;
// P2 for last APDU to receive.
const P2_SIGN_TX_LAST: u8 = 0x00;
// P2 for more APDU to receive.
const P2_SIGN_TX_MORE: u8 = 0x80;
// P1 for first APDU number.
const P1_SIGN_TX_START: u8 = 0x00;
// P1 for maximum APDU number.
const P1_SIGN_TX_MAX: u8 = 0x03;

// Application status words.
#[repr(u16)]
#[derive(Clone, Copy, PartialEq)]
pub enum AppSW {
    Deny = 0x6985,
    WrongP1P2 = 0x6A86,
    InsNotSupported = 0x6D00,
    ClaNotSupported = 0x6E00,
    TxDisplayFail = 0xB001,
    AddrDisplayFail = 0xB002,
    TxWrongLength = 0xB004,
    TxParsingFail = 0xB005,
    TxHashFail = 0xB006,
    TxSignFail = 0xB008,
    KeyDeriveFail = 0xB009,
    VersionParsingFail = 0xB00A,
    WrongApduLength = StatusWords::BadLen as u16,
    Ok = 0x9000,
}

impl From<AppSW> for Reply {
    fn from(sw: AppSW) -> Reply {
        Reply(sw as u16)
    }
}

/// Possible input commands received through APDUs.
pub enum Instruction {
    GetVersion,
    GetAppName,
    GetPubkey { display: bool },
    SignTx { chunk: u8, more: bool },
}

impl TryFrom<ApduHeader> for Instruction {
    type Error = AppSW;

    /// APDU parsing logic.
    ///
    /// Parses INS, P1 and P2 bytes to build an [`Instruction`]. P1 and P2 are translated to
    /// strongly typed variables depending on the APDU instruction code. Invalid INS, P1 or P2
    /// values result in errors with a status word, which are automatically sent to the host by the
    /// SDK.
    ///
    /// This design allows a clear separation of the APDU parsing logic and commands handling.
    ///
    /// Note that CLA is not checked here. Instead the method [`Comm::set_expected_cla`] is used in
    /// [`sample_main`] to have this verification automatically performed by the SDK.
    fn try_from(value: ApduHeader) -> Result<Self, Self::Error> {
        match (value.ins, value.p1, value.p2) {
            (3, 0, 0) => Ok(Instruction::GetVersion),
            (4, 0, 0) => Ok(Instruction::GetAppName),
            (5, 0 | 1, 0) => Ok(Instruction::GetPubkey {
                display: value.p1 != 0,
            }),
            (6, P1_SIGN_TX_START, P2_SIGN_TX_MORE)
            | (6, 1..=P1_SIGN_TX_MAX, P2_SIGN_TX_LAST | P2_SIGN_TX_MORE) => {
                Ok(Instruction::SignTx {
                    chunk: value.p1,
                    more: value.p2 == P2_SIGN_TX_MORE,
                })
            }
            (3..=6, _, _) => Err(AppSW::WrongP1P2),
            (_, _, _) => Err(AppSW::InsNotSupported),
        }
    }
}

fn show_status_and_home_if_needed(ins: &Instruction, tx_ctx: &mut TxContext, status: &AppSW) {
    let (show_status, status_type) = match (ins, status) {
        (Instruction::GetPubkey { display: true }, AppSW::Deny | AppSW::Ok) => {
            (true, StatusType::Address)
        }
        (Instruction::SignTx { .. }, AppSW::Deny | AppSW::Ok) if tx_ctx.finished() => {
            (true, StatusType::Transaction)
        }
        (_, _) => (false, StatusType::Transaction),
    };

    if show_status {
        let success = *status == AppSW::Ok;
        NbglReviewStatus::new()
            .status_type(status_type)
            .show(success);

        // call home.show_and_return() to show home and setting screen
        tx_ctx.home.show_and_return();
    }
}

#[no_mangle]
extern "C" fn sample_main() {
    // Create the communication manager, and configure it to accept only APDU from the 0xe0 class.
    // If any APDU with a wrong class value is received, comm will respond automatically with
    // BadCla status word.
    let mut comm = Comm::new().set_expected_cla(0xe0);
    init_comm(&mut comm);

    let mut tx_ctx = TxContext::new();

    let mut loop_count: i32 = 0;
    let mut status = true;
    loop {
        
        // let ins: Instruction = comm.next_command();

        // let _status = match handle_apdu(&mut comm, &ins, &mut tx_ctx) {
        //     Ok(()) => {
        //         comm.reply_ok();
        //         AppSW::Ok
        //     }
        //     Err(sw) => {
        //         comm.reply(sw);
        //         sw
        //     }
        // };        
        let msg = format!("Hello, time: {:?}!", loop_count);
        //#[cfg(any(target_os = "stax", target_os = "flex"))]

        
        const ZERO: NbglGlyph = NbglGlyph::from_include(include_gif!("glyphs/0.gif", NBGL));
        const ONE: NbglGlyph = NbglGlyph::from_include(include_gif!("glyphs/1.gif", NBGL));
        const TWO: NbglGlyph = NbglGlyph::from_include(include_gif!("glyphs/2.gif", NBGL));
        const THREE: NbglGlyph = NbglGlyph::from_include(include_gif!("glyphs/3.gif", NBGL));
        const FOUR: NbglGlyph = NbglGlyph::from_include(include_gif!("glyphs/4.gif", NBGL));
        const FIVE: NbglGlyph = NbglGlyph::from_include(include_gif!("glyphs/5.gif", NBGL));
        const SIX: NbglGlyph = NbglGlyph::from_include(include_gif!("glyphs/6.gif", NBGL));
        const SEVEN: NbglGlyph = NbglGlyph::from_include(include_gif!("glyphs/7.gif", NBGL));
        const EIGHT: NbglGlyph = NbglGlyph::from_include(include_gif!("glyphs/8.gif", NBGL));
        const NINE: NbglGlyph = NbglGlyph::from_include(include_gif!("glyphs/9.gif", NBGL));
        
        let  get_glyph = | number: i32 | {
            match number {
                0 => &ZERO,
                1 => &ONE,
                2 => &TWO,
                3 => &THREE,
                4 => &FOUR,
                5 => &FIVE,
                6 => &SIX,
                7 => &SEVEN,
                8 => &EIGHT,
                9 => &NINE,
                _ => &ZERO,
            }
        };
        let mut minutes: i32 = 5;
        let mut hours: i32 = 15; 

        

        
        status = !status;        
        loop {
            let mut icoH1 = get_glyph(hours / 10);
            let mut icoH2 = get_glyph(hours % 10);
            let mut icoM1 = get_glyph(minutes / 10);
            let mut icoM2 = get_glyph(minutes % 10);
            NbglClock::new().show(status, icoH1, icoH2, icoM1, icoM2);
            //comm.next_event::<ApduHeader>();
            loop_count += 1;        
            minutes += 1;
            if minutes >= 60 {
                minutes = 0;
                hours += 1;
                if hours >= 24 {
                    hours = 0;
                }
            }
            // match self.ux_sync_wait(true) {
            //         SyncNbgl::UxSyncRetApduReceived => {
            //             if let Some(hdr) = nbgl_fetch_apdu_header() {
            //                 // Reconstruct minimal Event::Command using APDU header only.
            //                 // The generic parameter T: TryFrom<ApduHeader> will parse header.
            //                 if let Ok(ins) = T::try_from(hdr) {
            //                     return Event::Command(ins);
            //                 } else {
            //                     // In case of parse error we emulate a BadIns reply.
            //                     nbgl_reply_status(Reply(StatusWords::BadIns as u16));
            //                 }
            //             }
            //         }
            //         SyncNbgl::UxSyncRetQuitted => {
            //             exit_app(0);
            //         }
            //         _ => {
            //             panic!("Unexpected return value from ux_sync_homeAndSettings");
            //         }
            //     }
        }
    }
}

fn handle_apdu(comm: &mut Comm, ins: &Instruction, ctx: &mut TxContext) -> Result<(), AppSW> {
    match ins {
        Instruction::GetAppName => {
            comm.append(env!("CARGO_PKG_NAME").as_bytes());
            Ok(())
        }
        Instruction::GetVersion => handler_get_version(comm),
        Instruction::GetPubkey { display } => handler_get_public_key(comm, *display),
        Instruction::SignTx { chunk, more } => handler_sign_tx(comm, *chunk, *more, ctx),
    }
}
