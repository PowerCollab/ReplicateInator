pub mod connection;
pub mod client;
pub mod server;
pub mod messaging;
pub mod authentication;

#[derive(Eq, PartialEq, Debug, Clone, Copy)]
pub enum OrderOptions{
    LittleEndian,
    BigEndian
}

#[derive(Debug, Clone, Copy)]
pub enum BytesOptions {
    U8,
    U16,
    U32,
    U64,
    U128,

    I8,
    I16,
    I32,
    I64,
    I128,

    F32,
    F64,
}

#[derive(Debug)]
pub enum ReadValue {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),

    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),

    F32(f32),
    F64(f64),
}