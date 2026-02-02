use embassy_rp::flash::Blocking;
use embassy_rp::flash::Flash;
use embassy_rp::peripherals::FLASH;
use heapless::String;

pub fn read_unique_id(flash: FLASH) -> String<32> {
    let mut flash = Flash::<_, Blocking, { 2 * 1024 * 1024 }>::new_blocking(flash);
    let mut uid = [0u8; 8];
    flash.blocking_unique_id(&mut uid).unwrap();

    let mut hex_uid = String::new();
    for &b in &uid {
        use core::fmt::Write;
        write!(&mut hex_uid, "{:02X}", b).unwrap();
    }
    hex_uid
}
