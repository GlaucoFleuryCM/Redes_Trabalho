/*
file contém: definição do protocolo;
*/

/*
bibliotecas usadas: nom (parsing binário)
*/
use nom::{
  IResult,
  bits::{bits, complete::take}
};

/*
funções pra decodificar e codificar mensagens binárias;
-> 'encode': objeto retorna a si mesmo em BIN
-> 'decode': objeto recebe um BIN (do seu tipo) e é preenchido com informação
*/
pub trait EncodeDecode {
    fn encode(&self) -> Vec<u8>;
    fn decode(input: &[u8]) -> IResult<&[u8], Self>
    where
        Self: Sized;
}

/*
struct que representa o header; tem como decodificar e codificar ela;
*/

// enum que diz qual é o tipo do payload
#[derive(PartialEq, Eq, Copy, Clone)]
pub enum MessageType {
    CONNECT,
    SensorData,
    ActCmd,
    SensorQuery,
    SensorRes,
    CONFIG
}

pub struct Header {
    pub magic_number: u32,
    pub version: u8,
    pub ack: bool,
    pub reserved: u8,
    pub kind: MessageType,
    pub length: u16
}

impl EncodeDecode for Header {
    fn encode(&self) -> Vec<u8> {
        /* a variável 'value' vai sendo preenchida pelos campos
        '[ magic | version | ack | reserved | kind | length ]',
        e aí no fim da função vira binário; */
        let mut value: u64 = 0;

        /* abaixo, os '<<' põem os pedaços binários nas posições certas do header;
        a máscara '0b111' pega os bits menos significativos de um número (ex: o
        campo 'kind' tem 4 bits, mas tá guardado num u64); */

        // 32 bits -> magic number
        value |= (self.magic_number as u64) << 32;
        // 8 bits -> version
        value |= (self.version as u64) << 24;
        // 1 bit -> ack
        let ack_bit = if self.ack { 1u64 } else { 0 };
        value |= ack_bit << 23;
        // 3 bits -> reserved
        value |= ((self.reserved as u64) & 0b111) << 20;
        // 4 bits -> kind
        let kind = match self.kind {
            MessageType::CONNECT => 0,
            MessageType::SensorData => 1,
            MessageType::ActCmd => 2,
            MessageType::SensorQuery => 3,
            MessageType::SensorRes => 4,
            MessageType::CONFIG => 5,
        };
        value |= (kind as u64 & 0b1111) << 16;
        // 16 bits -> length
        value |= self.length as u64;

        // bytes ficam big-endian (network byte order)
        value.to_be_bytes().to_vec()
    }

    fn decode(input: &[u8]) -> IResult<&[u8], Self> {
        bits(|input| {
            /* aqui vai lendo os bits do binário da mensagem e jogando nos
            campos correspondentes; */
            let (input, magic): (_, u32) = take(32usize)(input)?;
            let (input, version): (_, u8) = take(8usize)(input)?;
            let (input, ack_bit): (_, u8) = take(1usize)(input)?;
            let (input, reserved): (_, u8) = take(3usize)(input)?;
            let (input, kind_raw): (_, u8) = take(4usize)(input)?;
            let (input, length): (_, u16) = take(16usize)(input)?;

            // transforma o 'ack_bit' num bool (basicamente um cast);
            let ack = ack_bit != 0;

            // descobre o tipo da mensagem;
            let kind = match kind_raw {
                0 => MessageType::CONNECT,
                1 => MessageType::SensorData,
                2 => MessageType::ActCmd,
                3 => MessageType::SensorQuery,
                4 => MessageType::SensorRes,
                5 => MessageType::CONFIG,
                // rejeita mensagem que não tá definida no nosso escopo;
                _ => return Err(nom::Err::Failure(
                    nom::error::Error::new(input, nom::error::ErrorKind::Switch)
                )),
            };

            // rejeita protocolo que não seja o nosso no trabalho;
            if magic != u32::from_be_bytes(*b"PPPP") {
                return Err(nom::Err::Failure(
                    nom::error::Error::new(input, nom::error::ErrorKind::Tag)
                ));
            }

            // preenche nossa struct 'Header';
            let header = Header {
                magic_number: magic,
                version,
                ack,
                reserved,
                kind,
                length,
            };

            Ok((input, header))
        })(input)
    }
}

/*
structs de cada tipo de mensagem do relatório (CONNECT, ACT_CMD, etc); pra cada
struct, tem como decodificar e codificar ela;

a ideia é sempre parecida:
-> decode: lê 'X' bits e guarda numa variável de tamanho >= 'X' bits, com nome
   que diz o significado dos bits;
-> encode: cria um vetor 'v' e vai botando byte a byte a info a ser codificada;
*/

/* -> CONNECT <- */
pub struct Connect {
    pub kind: u8,
    pub id: u8
}

impl EncodeDecode for Connect {
    fn encode(&self) -> Vec<u8> {
        vec![self.kind, self.id]
    }

    fn decode(input: &[u8]) -> IResult<&[u8], Self> {
        let (input, kind) = nom::number::complete::be_u8(input)?;
        let (input, id) = nom::number::complete::be_u8(input)?;

        Ok((input, Connect { kind, id }))
    }
}

/* -> SENSOR_DATA <- */
pub struct SensorData {
    pub sensor_id: u8, // 1 byte conforme o protocolo
    pub value: f32,
}

impl EncodeDecode for SensorData {
    fn encode(&self) -> Vec<u8> {
        let mut v = Vec::new();
        v.push(self.sensor_id);
        v.extend(self.value.to_be_bytes());
        v
    }

    fn decode(input: &[u8]) -> IResult<&[u8], Self> {
        let (input, sensor_id) = nom::number::complete::be_u8(input)?;
        let (input, value_bytes) = nom::bytes::complete::take(4usize)(input)?;
        let value = f32::from_be_bytes(value_bytes.try_into().unwrap());
        Ok((input, SensorData {
            sensor_id,
            value
        }))
    }
}

/* -> ACT_CMD <- */
pub struct ActCmd {
    pub command: u8,
}

impl EncodeDecode for ActCmd {
    fn encode(&self) -> Vec<u8> {
        vec![self.command]
    }

    fn decode(input: &[u8]) -> IResult<&[u8], Self> {
        let (input, command) = nom::number::complete::be_u8(input)?;
        Ok((input, ActCmd { command }))
    }
}

/* -> SENSOR_QUERY <- */
pub struct SensorQuery {
    pub sensor_id: u8, // 1 byte conforme o protocolo
}

impl EncodeDecode for SensorQuery {
    fn encode(&self) -> Vec<u8> {
        vec![self.sensor_id]
    }

    fn decode(input: &[u8]) -> IResult<&[u8], Self> {
        let (input, sensor_id) = nom::number::complete::be_u8(input)?;
        Ok((input, SensorQuery {
            sensor_id
        }))
    }
}

/* -> SENSOR_RES <- */
pub struct SensorRes {
    pub sensor_id: u8, // 1 byte conforme o protocolo
    pub value: f32,
}

impl EncodeDecode for SensorRes {
    fn encode(&self) -> Vec<u8> {
        let mut v = Vec::new();
        v.push(self.sensor_id);
        v.extend(self.value.to_be_bytes());
        v
    }

    fn decode(input: &[u8]) -> IResult<&[u8], Self> {
        let (input, sensor_id) = nom::number::complete::be_u8(input)?;
        let (input, value_bytes) = nom::bytes::complete::take(4usize)(input)?;
        let value = f32::from_be_bytes(value_bytes.try_into().unwrap());
        Ok((input, SensorRes {
            sensor_id,
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
        v.extend(self.value.to_be_bytes());
        v
    }

    fn decode(input: &[u8]) -> IResult<&[u8], Self> {
        let (input, key) = nom::number::complete::be_u8(input)?;
        let (input, value_bytes) = nom::bytes::complete::take(4usize)(input)?;
        let value = f32::from_be_bytes(value_bytes.try_into().unwrap());
        Ok((input, Config {
            key,
            value
        }))
    }
}

/*
a struct message tem obrigatoriamente um header, e opcionalmente um payload
(acks não têm payload); o payload é um dos 6 tipos definidos no protocolo;
*/

pub enum Payload {
    Connect(Connect),
    SensorData(SensorData),
    ActCmd(ActCmd),
    SensorQuery(SensorQuery),
    SensorRes(SensorRes),
    Config(Config),
}

impl Payload {
    // tamanho fixo em bytes de cada variante, evita encodar só pra saber o tamanho
    fn encoded_len(&self) -> u16 {
        match self {
            Payload::Connect(_)     => 2,
            Payload::SensorData(_)  => 5,
            Payload::ActCmd(_)      => 1,
            Payload::SensorQuery(_) => 1,
            Payload::SensorRes(_)   => 5,
            Payload::Config(_)      => 5,
        }
    }
}

pub struct Message {
    pub header: Header,
    pub payload: Option<Payload>,
}

// encode: Message --> BIN (Vec<u8>);
// decode: BIN --> Message;
impl EncodeDecode for Message {
    fn encode(&self) -> Vec<u8> {
        let mut payload_bytes = Vec::new();

        /* vê se tem payload ou não, e se tiver, manda codificar aquele tipo
        específico de payload (CONNECT, CONFIG, etc); */
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

        // encoda o header;
        let mut header = self.header.encode();
        header.extend(payload_bytes);
        header
    }

    fn decode(input: &[u8]) -> IResult<&[u8], Self> {
        // decodifica o header
        let (input, header) = Header::decode(input)?;

        // se for um ack, esquece o payload
        if header.ack {
            return Ok((input, Message {
                header,
                payload: None
            }));
        }

        // identifica o tipo de payload e decodifica ele
        let (input, payload) = match header.kind {
            MessageType::CONNECT => {
                let (i, p) = Connect::decode(input)?;
                (i, Payload::Connect(p))
            }
            MessageType::SensorData => {
                let (i, p) = SensorData::decode(input)?;
                (i, Payload::SensorData(p))
            }
            MessageType::ActCmd => {
                let (i, p) = ActCmd::decode(input)?;
                (i, Payload::ActCmd(p))
            }
            MessageType::SensorQuery => {
                let (i, p) = SensorQuery::decode(input)?;
                (i, Payload::SensorQuery(p))
            }
            MessageType::SensorRes => {
                let (i, p) = SensorRes::decode(input)?;
                (i, Payload::SensorRes(p))
            }
            MessageType::CONFIG => {
                let (i, p) = Config::decode(input)?;
                (i, Payload::Config(p))
            }
        };

        // devolve a struct Message, com header e (se tiver) o payload
        Ok((input, Message {
            header,
            payload: Some(payload)
        }))
    }
}

/* a função 'try_decode' é um wrapper em cima do 'decode'; ela existe pra lidar
com a possibilidade do servidor ler mensagens ainda incompletas */
impl Message {
    // monta uma mensagem com o header preenchido automático, sem boilerplate
    pub fn new(kind: MessageType, payload: Payload) -> Message {
        let length = payload.encoded_len();
        Message {
            header: Header {
                magic_number: u32::from_be_bytes(*b"PPPP"),
                version: 1,
                ack: false,
                reserved: 0,
                kind,
                length,
            },
            payload: Some(payload),
        }
    }

    // monta um ack pro tipo indicado, sem payload
    pub fn ack(kind: MessageType) -> Message {
        Message {
            header: Header {
                magic_number: u32::from_be_bytes(*b"PPPP"),
                version: 1,
                ack: true,
                reserved: 0,
                kind,
                length: 0,
            },
            payload: None,
        }
    }

    pub fn try_decode(input: &[u8]) -> Option<(Message, usize)> {
        match Message::decode(input) {
            Ok((rest, msg)) => {
                let consumed = input.len() - rest.len();
                Some((msg, consumed))
            }
            Err(nom::Err::Incomplete(_)) => {
                // ainda não tem dado suficiente; espera e lê de novo
                None
            }
            Err(nom::Err::Error(_)) => {
                // erro recuperável, pode ser que mais dados resolvam
                None
            }
            Err(nom::Err::Failure(e)) => {
                // erro irrecuperável, isso é problema sério no protocolo;
                // panicar é a melhor opção pra deixar o erro visível
                panic!("Erro irrecuperável ao decodificar mensagem: {:?}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // o tamanho que o construtor declara tem que bater com o encode de verdade
    #[test]
    fn encoded_len_matches_real_encode() {
        let cases: Vec<Payload> = vec![
            Payload::Connect(Connect { kind: 0, id: 1 }),
            Payload::SensorData(SensorData { sensor_id: 0, value: 1.5 }),
            Payload::ActCmd(ActCmd { command: 1 }),
            Payload::SensorQuery(SensorQuery { sensor_id: 2 }),
            Payload::SensorRes(SensorRes { sensor_id: 2, value: 9.9 }),
            Payload::Config(Config { key: 4, value: 80.0 }),
        ];
        for p in cases {
            let real = match &p {
                Payload::Connect(x) => x.encode().len(),
                Payload::SensorData(x) => x.encode().len(),
                Payload::ActCmd(x) => x.encode().len(),
                Payload::SensorQuery(x) => x.encode().len(),
                Payload::SensorRes(x) => x.encode().len(),
                Payload::Config(x) => x.encode().len(),
            };
            assert_eq!(p.encoded_len() as usize, real);
        }
    }

    // header sozinho: encoda e decoda mantendo todos os campos
    #[test]
    fn header_round_trip() {
        let h = Header {
            magic_number: u32::from_be_bytes(*b"PPPP"),
            version: 1,
            ack: true,
            reserved: 5,
            kind: MessageType::SensorRes,
            length: 5,
        };
        let bytes = h.encode();
        assert_eq!(bytes.len(), 8); // header sempre tem 8 bytes
        let (_, dec) = Header::decode(&bytes).unwrap();
        assert_eq!(dec.magic_number, h.magic_number);
        assert_eq!(dec.version, 1);
        assert!(dec.ack);
        assert_eq!(dec.reserved, 5);
        assert!(matches!(dec.kind, MessageType::SensorRes));
        assert_eq!(dec.length, 5);
    }

    // mensagem completa com payload: round trip preserva os campos do payload
    #[test]
    fn message_sensor_data_round_trip() {
        let msg = Message::new(MessageType::SensorData, Payload::SensorData(SensorData { sensor_id: 2, value: 42.25 }));
        let bytes = msg.encode();
        let (dec, consumed) = Message::try_decode(&bytes).expect("devia decodar");
        assert_eq!(consumed, bytes.len());
        match dec.payload {
            Some(Payload::SensorData(sd)) => {
                assert_eq!(sd.sensor_id, 2);
                assert_eq!(sd.value, 42.25);
            }
            _ => panic!("payload errado"),
        }
    }

    // construtor new preenche o header certo (magic, version, sem ack, length)
    #[test]
    fn new_fills_header() {
        let msg = Message::new(MessageType::CONFIG, Payload::Config(Config { key: 0, value: 1.0 }));
        assert_eq!(msg.header.magic_number, u32::from_be_bytes(*b"PPPP"));
        assert_eq!(msg.header.version, 1);
        assert!(!msg.header.ack);
        assert_eq!(msg.header.length, 5); // config = 1 byte key + 4 bytes float
    }

    // ack não tem payload e decoda como ack sem payload
    #[test]
    fn ack_has_no_payload() {
        let msg = Message::ack(MessageType::CONNECT);
        assert!(msg.header.ack);
        assert!(msg.payload.is_none());
        let bytes = msg.encode();
        let (dec, _) = Message::try_decode(&bytes).unwrap();
        assert!(dec.header.ack);
        assert!(dec.payload.is_none()); // decode pula o payload quando é ack
    }

    // mensagem cortada no meio não decoda, devolve None em vez de panicar
    #[test]
    fn incomplete_message_returns_none() {
        let msg = Message::new(MessageType::SensorData, Payload::SensorData(SensorData { sensor_id: 0, value: 1.0 }));
        let bytes = msg.encode();
        let cut = &bytes[..bytes.len() - 1]; // tira o último byte do payload
        assert!(Message::try_decode(cut).is_none());
    }

    // duas mensagens grudadas: try_decode consome só a primeira
    #[test]
    fn try_decode_consumes_one_at_a_time() {
        let m1 = Message::new(MessageType::ActCmd, Payload::ActCmd(ActCmd { command: 1 })).encode();
        let m2 = Message::new(MessageType::ActCmd, Payload::ActCmd(ActCmd { command: 0 })).encode();
        let mut buf = m1.clone();
        buf.extend_from_slice(&m2);
        let (_, consumed) = Message::try_decode(&buf).unwrap();
        assert_eq!(consumed, m1.len()); // sobra a segunda mensagem no buffer
        let (_, c2) = Message::try_decode(&buf[consumed..]).unwrap();
        assert_eq!(c2, m2.len());
    }

    // magic number errado é rejeitado no decode
    #[test]
    fn wrong_magic_fails() {
        let mut bytes = Message::new(MessageType::ActCmd, Payload::ActCmd(ActCmd { command: 1 })).encode();
        bytes[0] = b'X'; // estraga o magic "PPPP"
        assert!(Message::decode(&bytes).is_err());
    }
}
