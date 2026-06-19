/*
File contém: definição do Protocolo; 
*/

/*
Bibliotecas utilizadas: nom
*/
use nom::{
  IResult,
  Parser,
  bits::{bits, complete::take},
  bytes::complete::{tag, take_while_m_n},
  combinator::map_res
};

/* 
funções p/decodificar e codificar mensagens
binárias;
-> 'encode': objeto retorna a si mesmo em BIN
-> 'decode': objeto recebe um BIN (de seu tipo)
    e é preenchido com informação
*/
pub trait EncodeDecode {
    fn encode(&self) -> Vec<u8>;
    fn decode(input: &[u8]) -> IResult<&[u8], Self>
    where
        Self: Sized;
}

/* 
struct que representa o Header; é especificado como
decodificar e codificá-la;
*/

pub enum TipoMensagem {
    CONNECT,
    SENSOR_DATA,
    ACT_CMD,
    SENSOR_QUERY,
    SENSOR_RES,
    CONFIG
}

pub struct Header {
    pub magic_number: u32, 
    pub versao: u8,
    pub ack: bool,
    pub reserved: u8,
    pub tipo: TipoMensagem,
    pub tamanho: u16
}

impl EncodeDecode for Header {
    fn encode(&self) -> Vec<u8> {
        let mut value: u64 = 0;

        // 32 bits -> magic number
        value |= (self.magic_number as u64) << 32;
        // 8 bits -> version
        value |= (self.versao as u64) << 24;
        // 1 bit -> ack
        let ack_bit = if self.ack { 1u64 } else { 0 };
        value |= ack_bit << 23;
        // 3 bits -> reserved
        value |= ((self.reserved as u64) & 0b111) << 20;
        // 4 bits -> tipo
        let tipo = match self.tipo {
            TipoMensagem::CONNECT => 0,
            TipoMensagem::SENSOR_DATA => 1,
            TipoMensagem::ACT_CMD => 2,
            TipoMensagem::SENSOR_QUERY => 3,
            TipoMensagem::SENSOR_RES => 4,
            TipoMensagem::CONFIG => 5,
        };
        value |= (tipo as u64 & 0b1111) << 16;
        // 16 bits -> tamanho
        value |= self.tamanho as u64;

        // bytes ficam big-endian
        value.to_be_bytes().to_vec()
    }

    fn decode(input: &[u8]) -> IResult<&[u8], Self> {
        bits(|input| {
            let (input, magic): (_, u32) = take(32usize)(input)?;
            let (input, versao): (_, u8) = take(8usize)(input)?;
            let (input, ack_bit): (_, u8) = take(1usize)(input)?;
            let (input, reserved): (_, u8) = take(3usize)(input)?;
            let (input, tipo_raw): (_, u8) = take(4usize)(input)?;
            let (input, tamanho): (_, u16) = take(16usize)(input)?;

            let ack = ack_bit != 0;
             
            let tipo = match tipo_raw {
                0 => TipoMensagem::CONNECT,
                1 => TipoMensagem::SENSOR_DATA,
                2 => TipoMensagem::ACT_CMD,
                3 => TipoMensagem::SENSOR_QUERY,
                4 => TipoMensagem::SENSOR_RES,
                5 => TipoMensagem::CONFIG,
                // rejeita mensagens que não sejam definidas no nosso escopo
                _ => return Err(nom::Err::Failure(
                    nom::error::Error::new(input, nom::error::ErrorKind::Switch)
                )),
            };

            // rejeita protocolos que não sejam o nosso no trabalho 
            if magic != u32::from_be_bytes(*b"PPPP") {
                return Err(nom::Err::Failure(
                    nom::error::Error::new(input, nom::error::ErrorKind::Tag)
                ));
            }

            /* preenche nossa Struct 'Header' */
            let header = Header {
                magic_number: magic,
                versao,
                ack,
                reserved,
                tipo,
                tamanho,
            };

            Ok((input, header))
        })(input)
    }
}

/*
structs que definem cada tipo de mensagem especificada
no relatório (CONNECT, ACT_CMD, etc); para cada struct,
é especificado como decoficar e codificar ela;
*/

/* -> CONNECT <- */
pub struct Connect {
    pub tipo: u8,
    pub id: u8
}

impl EncodeDecode for Connect {
    fn encode(&self) -> Vec<u8> {
        vec![self.tipo, self.id]
    }

    fn decode(input: &[u8]) -> IResult<&[u8], Self> {
        let (input, tipo) = nom::number::complete::be_u8(input)?;
        let (input, id) = nom::number::complete::be_u8(input)?;

        Ok((input, Connect { tipo, id }))
    }
}

/* -> SENSOR_DATA <- */
pub struct SensorData {
    pub sensor_id: u32,
    pub value: f32,
}

impl EncodeDecode for SensorData {
    fn encode(&self) -> Vec<u8> {
        let mut v = Vec::new();
        v.push(self.sensor_id as u8);
        v.extend(self.value.to_be_bytes());
        v
    }

    fn decode(input: &[u8]) -> IResult<&[u8], Self> {
        let (input, sensor_id) = nom::number::complete::be_u8(input)?;
        let (input, value_bytes) = nom::bytes::complete::take(4usize)(input)?;
        let value = f32::from_be_bytes(value_bytes.try_into().unwrap());
        Ok((input, SensorData {
            sensor_id: sensor_id as u32,
            value
        }))
    }
}

/* -> ACT_CMD <- */
pub struct ActCmd {
    pub actuator_id: u32,
    pub command: u8,
}

impl EncodeDecode for ActCmd {
    fn encode(&self) -> Vec<u8> {
        vec![self.command]
    }

    fn decode(input: &[u8]) -> IResult<&[u8], Self> {
        let (input, command) = nom::number::complete::be_u8(input)?;
        Ok((input, ActCmd {
            actuator_id: 0, // não existe no protocolo
            command
        }))
    }
}

/* -> SENSOR_QUERY <- */
pub struct SensorQuery {
    pub sensor_id: u32,
}

impl EncodeDecode for SensorQuery {
    fn encode(&self) -> Vec<u8> {
        vec![self.sensor_id as u8]
    }

    fn decode(input: &[u8]) -> IResult<&[u8], Self> {
        let (input, sensor_id) = nom::number::complete::be_u8(input)?;
        Ok((input, SensorQuery {
            sensor_id: sensor_id as u32
        }))
    }
}

/* -> SENSOR_RES <- */
pub struct SensorRes {
    pub sensor_id: u32,
    pub value: f32,
}

impl EncodeDecode for SensorRes {
    fn encode(&self) -> Vec<u8> {
        let mut v = Vec::new();
        v.push(self.sensor_id as u8);
        v.extend(self.value.to_be_bytes());
        v
    }

    fn decode(input: &[u8]) -> IResult<&[u8], Self> {
        let (input, sensor_id) = nom::number::complete::be_u8(input)?;
        let (input, value_bytes) = nom::bytes::complete::take(4usize)(input)?;
        let value = f32::from_be_bytes(value_bytes.try_into().unwrap());
        Ok((input, SensorRes {
            sensor_id: sensor_id as u32,
            value
        }))
    }
}

/* -> CONFIG <- */
pub struct Config {
    pub key: u8,
    pub value: f32,
}

impl EncodeDecode for Config {
    fn encode(&self) -> Vec<u8> {
        let mut v = Vec::new();
        v.push(self.key);
        v.extend((self.value as f32).to_be_bytes());
        v
    }

    fn decode(input: &[u8]) -> IResult<&[u8], Self> {
        let (input, key) = nom::number::complete::be_u8(input)?;
        let (input, value_bytes) = nom::bytes::complete::take(4usize)(input)?;
        let value = f32::from_be_bytes(value_bytes.try_into().unwrap());
        Ok((input, Config {
            key,
            value: value as f32
        }))
    }
}

/* 
A struct mensagem contém obrigatoriamente um header,
e opcionalmente um payload (ACKs não tem payload); o
payload é um dos 6 tipos definidos no protocolo;
*/

pub enum Payload {
    Connect(Connect),
    SensorData(SensorData),
    ActCmd(ActCmd),
    SensorQuery(SensorQuery),
    SensorRes(SensorRes),
    Config(Config),
}

pub struct Mensagem {
    pub header: Header,
    pub payload: Option<Payload>, 
}

impl EncodeDecode for Mensagem {
    fn encode(&self) -> Vec<u8> {
        let mut payload_bytes = Vec::new();

        if let Some(payload) = &self.payload {
            payload_bytes = match payload {
                Payload::Connect(p) => p.encode(),
                Payload::SensorData(p) => p.encode(),
                Payload::ActCmd(p) => p.encode(),
                Payload::SensorQuery(p) => p.encode(),
                Payload::SensorRes(p) => p.encode(),
                Payload::Config(p) => p.encode(),
            };
        }

        let mut header = self.header.encode();
        header.extend(payload_bytes);
        header
    }

    fn decode(input: &[u8]) -> IResult<&[u8], Self> {
        let (input, header) = Header::decode(input)?;

        // ACK: sem payload
        if header.ack {
            return Ok((input, Mensagem {
                header,
                payload: None
            }));
        }

        let (input, payload) = match header.tipo {
            TipoMensagem::CONNECT => {
                let (i, p) = Connect::decode(input)?;
                (i, Payload::Connect(p))
            }
            TipoMensagem::SENSOR_DATA => {
                let (i, p) = SensorData::decode(input)?;
                (i, Payload::SensorData(p))
            }
            TipoMensagem::ACT_CMD => {
                let (i, p) = ActCmd::decode(input)?;
                (i, Payload::ActCmd(p))
            }
            TipoMensagem::SENSOR_QUERY => {
                let (i, p) = SensorQuery::decode(input)?;
                (i, Payload::SensorQuery(p))
            }
            TipoMensagem::SENSOR_RES => {
                let (i, p) = SensorRes::decode(input)?;
                (i, Payload::SensorRes(p))
            }
            TipoMensagem::CONFIG => {
                let (i, p) = Config::decode(input)?;
                (i, Payload::Config(p))
            }
        };

        Ok((input, Mensagem {
            header,
            payload: Some(payload)
        }))
    }
}
