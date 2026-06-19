mod protocolo;
use protocolo::*;

fn main() {
// =========================
// 1. Criando a mensagem
// =========================
let mensagem = Mensagem {
header: Header {
magic_number: u32::from_be_bytes(*b"PPPP"),
versao: 1,
ack: false,
reserved: 0,
tipo: TipoMensagem::CONFIG,
tamanho: 5, // 1 byte + 4 bytes
},
payload: Some(Payload::Config(Config {
key: 3,
value: 3.448,
})),
};

// =========================
// 2. Encode
// =========================
let encoded = mensagem.encode();

println!("Encoded bytes:");
for b in &encoded {
    print!("{:02X} ", b);
}
println!("\n");

// =========================
// 3. Decode
// =========================
let (_rest, decoded_msg) = Mensagem::decode(&encoded).unwrap();

// =========================
// 4. Verificação
// =========================
println!("Mensagem original:");
print_msg(&mensagem);

println!("\nMensagem decodificada:");
print_msg(&decoded_msg);

}

// helper só pra debug bonito
fn print_msg(msg: &Mensagem) {
println!("Header:");
println!(" magic: {:?}", msg.header.magic_number);
println!(" versao: {}", msg.header.versao);
println!(" ack: {}", msg.header.ack);
//println!(" tipo: {}", msg.header.tipo);
println!(" tamanho: {}", msg.header.tamanho);

match &msg.payload {
    Some(Payload::Config(c)) => {
        println!("Payload CONFIG:");
        println!(" key: {}", c.key);
        println!(" value: {}", c.value);
    }
    _ => println!("Outro payload"),
}

}
