/*
file contém: registro central dos dispositivos da estufa;
é a fonte única de verdade pra ids, nomes, arquivos de ambiente e taxas;
adicionar um dispositivo novo (que só mexe em dado) é só botar uma linha aqui;
*/

// descreve um sensor: o que mede, onde guarda o valor simulado, e o decaimento natural dele
pub struct SensorDesc {
    pub id: u8,
    pub name: &'static str,
    pub file: &'static str,
    pub initial_value: f32,
    pub decay: f32, // quanto cai por tick no ambiente, 0.0 = não decai sozinho
}

// descreve um atuador: o que ele faz no ambiente quando tá ligado
pub struct ActuatorDesc {
    pub id: u8,
    pub name: &'static str,
    pub file: &'static str,
    pub variation: f32, // quanto muda o ambiente por tick, positivo sobe, negativo desce
}

pub const SENSORS: &[SensorDesc] = &[
    // temp começa quente (acima do setpoint) pra cair direto no cooler; sem decaimento,
    // a oscilação vem do overshoot do aquecedor/resfriador batendo nas bordas da banda
    SensorDesc { id: 0, name: "temperatura", file: "src/env_vars/temp.txt", initial_value: 29.0,  decay: 0.0 },
    // hum e co2 decaem rápido, aí o irrigador/injetor ligam, enchem, desligam e repetem
    SensorDesc { id: 1, name: "umidade",     file: "src/env_vars/hum.txt",  initial_value: 50.0,  decay: 1.5 },
    SensorDesc { id: 2, name: "co2",         file: "src/env_vars/co2.txt",  initial_value: 400.0, decay: 3.0 },
];

pub const ACTUATORS: &[ActuatorDesc] = &[
    ActuatorDesc { id: 3, name: "aquecedor",      file: "src/env_vars/temp.txt", variation:  0.5 },
    ActuatorDesc { id: 4, name: "resfriador",     file: "src/env_vars/temp.txt", variation: -0.5 },
    ActuatorDesc { id: 5, name: "irrigador",      file: "src/env_vars/hum.txt",  variation:  1.0 },
    ActuatorDesc { id: 6, name: "injetor de co2", file: "src/env_vars/co2.txt",  variation:  2.0 },
];

// acha o descritor do sensor pelo id, None se não existe no registro
pub fn sensor_by_id(id: u8) -> Option<&'static SensorDesc> {
    SENSORS.iter().find(|s| s.id == id)
}

// acha o descritor do atuador pelo id, None se não existe no registro
pub fn actuator_by_id(id: u8) -> Option<&'static ActuatorDesc> {
    ACTUATORS.iter().find(|a| a.id == id)
}

// nome de qualquer dispositivo, sensor ou atuador, pra usar em log
pub fn name_by_id(id: u8) -> &'static str {
    sensor_by_id(id)
        .map(|s| s.name)
        .or_else(|| actuator_by_id(id).map(|a| a.name))
        .unwrap_or("desconhecido")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensor_and_actuator_ids_dont_overlap() {
        // se um id fosse sensor e atuador ao mesmo tempo, o roteamento quebraria
        for s in SENSORS {
            assert!(actuator_by_id(s.id).is_none(), "id {} é sensor e atuador", s.id);
        }
    }

    #[test]
    fn all_ids_are_unique() {
        let mut ids: Vec<u8> = SENSORS.iter().map(|s| s.id).collect();
        ids.extend(ACTUATORS.iter().map(|a| a.id));
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total, "tem id repetido no registro");
    }

    #[test]
    fn lookup_finds_and_misses_correctly() {
        assert_eq!(sensor_by_id(0).unwrap().name, "temperatura");
        assert_eq!(actuator_by_id(3).unwrap().name, "aquecedor");
        assert!(sensor_by_id(99).is_none());
        assert!(actuator_by_id(0).is_none()); // 0 é sensor, não atuador
    }

    #[test]
    fn name_by_id_covers_both_types_and_unknown() {
        assert_eq!(name_by_id(1), "umidade");
        assert_eq!(name_by_id(5), "irrigador");
        assert_eq!(name_by_id(200), "desconhecido");
    }
}
