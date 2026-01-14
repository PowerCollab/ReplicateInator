pub mod plugins;
pub mod ports;

#[derive(Default, PartialEq, Clone, Copy, Debug)]
pub enum NetworkSide{
    #[default]
    Server,
    Client
}

impl NetworkSide {
    pub const fn as_u8(self) -> u8 {
        match self {
            NetworkSide::Server => 0,
            NetworkSide::Client => 1,
        }
    }

    pub const fn from_u8(v: u8) -> Self {
        match v {
            1 => NetworkSide::Client,
            _ => NetworkSide::Server,
        }
    }
}
