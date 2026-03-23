pub fn print_hex_byte(byte: u8) {
    let hex = "0123456789ABCDEF";
    let high = (byte >> 4) as usize;
    let low = (byte & 0xF) as usize;
    crate::print_char(hex.chars().nth(high).unwrap());
    crate::print_char(hex.chars().nth(low).unwrap());
}

pub fn print_mac(mac: &[u8; 6]) {
    crate::println("VirtIO Net MAC:");
    for (i, &byte) in mac.iter().enumerate() {
        print_hex_byte(byte);
        if i < 5 {
            crate::print_char(':');
        }
    }
    crate::println(""); // Newline
}
