use std::io::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;
use crate::plugins::{BytesOptions, OrderOptions, ReadValue};

pub fn read_value_to_usize(value: ReadValue) -> usize {
    match value {
        // Unsigned
        ReadValue::U8(v) => v as usize,
        ReadValue::U16(v) => v as usize,
        ReadValue::U32(v) => v as usize,
        ReadValue::U64(v) => v as usize,
        ReadValue::U128(v) => v as usize,

        // Signed
        ReadValue::I8(v) => v as usize,
        ReadValue::I16(v) => v as usize,
        ReadValue::I32(v) => v as usize,
        ReadValue::I64(v) => v as usize,
        ReadValue::I128(v) => v as usize,

        // Floats
        ReadValue::F32(v) => v as usize,
        ReadValue::F64(v) => v as usize,
    }
}

pub fn value_from_number(number: f64, bytes: BytesOptions) -> ReadValue {
    match bytes {
        // Unsigned
        BytesOptions::U8 => ReadValue::U8(number as u8),
        BytesOptions::U16 => ReadValue::U16(number as u16),
        BytesOptions::U32 => ReadValue::U32(number as u32),
        BytesOptions::U64 => ReadValue::U64(number as u64),
        BytesOptions::U128 => ReadValue::U128(number as u128),

        // Signed
        BytesOptions::I8 => ReadValue::I8(number as i8),
        BytesOptions::I16 => ReadValue::I16(number as i16),
        BytesOptions::I32 => ReadValue::I32(number as i32),
        BytesOptions::I64 => ReadValue::I64(number as i64),
        BytesOptions::I128 => ReadValue::I128(number as i128),

        // Floats
        BytesOptions::F32 => ReadValue::F32(number as f32),
        BytesOptions::F64 => ReadValue::F64(number),
    }
}

pub async fn write_from_settings_not_owned(
    write_half: &mut WriteHalf<TlsStream<TcpStream>>,
    value: &ReadValue,
    order: &OrderOptions,
) -> Result<(), Error> {
    match value {
        // Unsigned
        ReadValue::U8(v) => write_half.write_u8(*v).await?,
        ReadValue::U16(v) => match order {
            OrderOptions::LittleEndian => write_half.write_u16_le(*v).await?,
            OrderOptions::BigEndian => write_half.write_u16(*v).await?,
        },
        ReadValue::U32(v) => match order {
            OrderOptions::LittleEndian => write_half.write_u32_le(*v).await?,
            OrderOptions::BigEndian => write_half.write_u32(*v).await?,
        },
        ReadValue::U64(v) => match order {
            OrderOptions::LittleEndian => write_half.write_u64_le(*v).await?,
            OrderOptions::BigEndian => write_half.write_u64(*v).await?,
        },
        ReadValue::U128(v) => match order {
            OrderOptions::LittleEndian => write_half.write_u128_le(*v).await?,
            OrderOptions::BigEndian => write_half.write_u128(*v).await?,
        },

        // Signed
        ReadValue::I8(v) => write_half.write_i8(*v).await?,
        ReadValue::I16(v) => match order {
            OrderOptions::LittleEndian => write_half.write_i16_le(*v).await?,
            OrderOptions::BigEndian => write_half.write_i16(*v).await?,
        },
        ReadValue::I32(v) => match order {
            OrderOptions::LittleEndian => write_half.write_i32_le(*v).await?,
            OrderOptions::BigEndian => write_half.write_i32(*v).await?,
        },
        ReadValue::I64(v) => match order {
            OrderOptions::LittleEndian => write_half.write_i64_le(*v).await?,
            OrderOptions::BigEndian => write_half.write_i64(*v).await?,
        },
        ReadValue::I128(v) => match order {
            OrderOptions::LittleEndian => write_half.write_i128_le(*v).await?,
            OrderOptions::BigEndian => write_half.write_i128(*v).await?,
        },

        // Floats
        ReadValue::F32(v) => match order {
            OrderOptions::LittleEndian => write_half.write_f32_le(*v).await?,
            OrderOptions::BigEndian => write_half.write_f32(*v).await?,
        },
        ReadValue::F64(v) => match order {
            OrderOptions::LittleEndian => write_half.write_f64_le(*v).await?,
            OrderOptions::BigEndian => write_half.write_f64(*v).await?,
        },
    }

    Ok(())
}

pub async fn write_from_settings(
    write_half: &mut OwnedWriteHalf,
    value: &ReadValue,
    order: &OrderOptions,
) -> Result<(), Error> {
    match value {
        // Unsigned
        ReadValue::U8(v) => write_half.write_u8(*v).await?,
        ReadValue::U16(v) => match order {
            OrderOptions::LittleEndian => write_half.write_u16_le(*v).await?,
            OrderOptions::BigEndian => write_half.write_u16(*v).await?,
        },
        ReadValue::U32(v) => match order {
            OrderOptions::LittleEndian => write_half.write_u32_le(*v).await?,
            OrderOptions::BigEndian => write_half.write_u32(*v).await?,
        },
        ReadValue::U64(v) => match order {
            OrderOptions::LittleEndian => write_half.write_u64_le(*v).await?,
            OrderOptions::BigEndian => write_half.write_u64(*v).await?,
        },
        ReadValue::U128(v) => match order {
            OrderOptions::LittleEndian => write_half.write_u128_le(*v).await?,
            OrderOptions::BigEndian => write_half.write_u128(*v).await?,
        },

        // Signed
        ReadValue::I8(v) => write_half.write_i8(*v).await?,
        ReadValue::I16(v) => match order {
            OrderOptions::LittleEndian => write_half.write_i16_le(*v).await?,
            OrderOptions::BigEndian => write_half.write_i16(*v).await?,
        },
        ReadValue::I32(v) => match order {
            OrderOptions::LittleEndian => write_half.write_i32_le(*v).await?,
            OrderOptions::BigEndian => write_half.write_i32(*v).await?,
        },
        ReadValue::I64(v) => match order {
            OrderOptions::LittleEndian => write_half.write_i64_le(*v).await?,
            OrderOptions::BigEndian => write_half.write_i64(*v).await?,
        },
        ReadValue::I128(v) => match order {
            OrderOptions::LittleEndian => write_half.write_i128_le(*v).await?,
            OrderOptions::BigEndian => write_half.write_i128(*v).await?,
        },

        // Floats
        ReadValue::F32(v) => match order {
            OrderOptions::LittleEndian => write_half.write_f32_le(*v).await?,
            OrderOptions::BigEndian => write_half.write_f32(*v).await?,
        },
        ReadValue::F64(v) => match order {
            OrderOptions::LittleEndian => write_half.write_f64_le(*v).await?,
            OrderOptions::BigEndian => write_half.write_f64(*v).await?,
        },
    }

    Ok(())
}

pub async fn read_from_settings_not_owned(
    read_half: &mut ReadHalf<TlsStream<TcpStream>>,
    bytes: &BytesOptions,
    order: &OrderOptions,
) -> Result<ReadValue, Error> {
    let value = match bytes {
        BytesOptions::U8 => ReadValue::U8(read_half.read_u8().await?),
        BytesOptions::U16 => ReadValue::U16(match order {
            OrderOptions::LittleEndian => read_half.read_u16_le().await?,
            OrderOptions::BigEndian => read_half.read_u16().await?,
        }),
        BytesOptions::U32 => ReadValue::U32(match order {
            OrderOptions::LittleEndian => read_half.read_u32_le().await?,
            OrderOptions::BigEndian => read_half.read_u32().await?,
        }),
        BytesOptions::U64 => ReadValue::U64(match order {
            OrderOptions::LittleEndian => read_half.read_u64_le().await?,
            OrderOptions::BigEndian => read_half.read_u64().await?,
        }),
        BytesOptions::U128 => ReadValue::U128(match order {
            OrderOptions::LittleEndian => read_half.read_u128_le().await?,
            OrderOptions::BigEndian => read_half.read_u128().await?,
        }),

        BytesOptions::I8 => ReadValue::I8(read_half.read_i8().await?),
        BytesOptions::I16 => ReadValue::I16(match order {
            OrderOptions::LittleEndian => read_half.read_i16_le().await?,
            OrderOptions::BigEndian => read_half.read_i16().await?,
        }),
        BytesOptions::I32 => ReadValue::I32(match order {
            OrderOptions::LittleEndian => read_half.read_i32_le().await?,
            OrderOptions::BigEndian => read_half.read_i32().await?,
        }),
        BytesOptions::I64 => ReadValue::I64(match order {
            OrderOptions::LittleEndian => read_half.read_i64_le().await?,
            OrderOptions::BigEndian => read_half.read_i64().await?,
        }),
        BytesOptions::I128 => ReadValue::I128(match order {
            OrderOptions::LittleEndian => read_half.read_i128_le().await?,
            OrderOptions::BigEndian => read_half.read_i128().await?,
        }),

        BytesOptions::F32 => ReadValue::F32(match order {
            OrderOptions::LittleEndian => read_half.read_f32_le().await?,
            OrderOptions::BigEndian => read_half.read_f32().await?,
        }),
        BytesOptions::F64 => ReadValue::F64(match order {
            OrderOptions::LittleEndian => read_half.read_f64_le().await?,
            OrderOptions::BigEndian => read_half.read_f64().await?,
        }),
    };

    Ok(value)
}

pub async fn read_from_settings(
    read_half: &mut OwnedReadHalf,
    bytes: &BytesOptions,
    order: &OrderOptions,
) -> Result<ReadValue, Error> {
    let value = match bytes {
        BytesOptions::U8 => ReadValue::U8(read_half.read_u8().await?),
        BytesOptions::U16 => ReadValue::U16(match order {
            OrderOptions::LittleEndian => read_half.read_u16_le().await?,
            OrderOptions::BigEndian => read_half.read_u16().await?,
        }),
        BytesOptions::U32 => ReadValue::U32(match order {
            OrderOptions::LittleEndian => read_half.read_u32_le().await?,
            OrderOptions::BigEndian => read_half.read_u32().await?,
        }),
        BytesOptions::U64 => ReadValue::U64(match order {
            OrderOptions::LittleEndian => read_half.read_u64_le().await?,
            OrderOptions::BigEndian => read_half.read_u64().await?,
        }),
        BytesOptions::U128 => ReadValue::U128(match order {
            OrderOptions::LittleEndian => read_half.read_u128_le().await?,
            OrderOptions::BigEndian => read_half.read_u128().await?,
        }),

        BytesOptions::I8 => ReadValue::I8(read_half.read_i8().await?),
        BytesOptions::I16 => ReadValue::I16(match order {
            OrderOptions::LittleEndian => read_half.read_i16_le().await?,
            OrderOptions::BigEndian => read_half.read_i16().await?,
        }),
        BytesOptions::I32 => ReadValue::I32(match order {
            OrderOptions::LittleEndian => read_half.read_i32_le().await?,
            OrderOptions::BigEndian => read_half.read_i32().await?,
        }),
        BytesOptions::I64 => ReadValue::I64(match order {
            OrderOptions::LittleEndian => read_half.read_i64_le().await?,
            OrderOptions::BigEndian => read_half.read_i64().await?,
        }),
        BytesOptions::I128 => ReadValue::I128(match order {
            OrderOptions::LittleEndian => read_half.read_i128_le().await?,
            OrderOptions::BigEndian => read_half.read_i128().await?,
        }),

        BytesOptions::F32 => ReadValue::F32(match order {
            OrderOptions::LittleEndian => read_half.read_f32_le().await?,
            OrderOptions::BigEndian => read_half.read_f32().await?,
        }),
        BytesOptions::F64 => ReadValue::F64(match order {
            OrderOptions::LittleEndian => read_half.read_f64_le().await?,
            OrderOptions::BigEndian => read_half.read_f64().await?,
        }),
    };

    Ok(value)
}