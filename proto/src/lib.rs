#[derive(Clone, Debug, PartialEq)]
pub enum ChatPacketType {
    Unknown,
    // client send login with name
    // server pass login with to all
    Login,
    // client send chat message
    // server pass chat message to all
    Chat,
    // client send close
    // server pass close to all
    Close,
}

impl From<u8> for ChatPacketType {
    fn from(byte: u8) -> Self {
        match byte {
            1 => Self::Login,
            2 => Self::Chat,
            3 => Self::Close,
            _ => Self::Unknown,
        }
    }
}

impl From<ChatPacketType> for u8 {
    fn from(packet_type: ChatPacketType) -> Self {
        match packet_type {
            ChatPacketType::Login => 1,
            ChatPacketType::Chat => 2,
            ChatPacketType::Close => 3,
            _ => 0,
        }
    }
}

#[derive(actix::Message, Clone, Debug, PartialEq)]
#[rtype(result = "()")]
pub struct ChatPacket {
    pub packet_type: ChatPacketType,
    pub packet_message: String,
}

impl ChatPacket {
    pub fn new(packet_type: ChatPacketType, packet_message: String) -> Self {
        Self {
            packet_type,
            packet_message,
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut serialized_packet = Vec::new();
        let packet_type: u8 = self.packet_type.to_owned().into();
        serialized_packet.push(packet_type);
        serialized_packet.extend(self.packet_message.as_bytes());
        serialized_packet
    }

    pub fn deserialize(packet: Vec<u8>) -> Self {
        let packet_type = ChatPacketType::from(packet[0]);
        let packet_message = String::from_utf8(packet[1..].to_vec()).unwrap();
        Self {
            packet_type,
            packet_message,
        }
    }
}
