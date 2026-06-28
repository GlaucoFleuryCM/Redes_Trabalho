# Project Analysis — SSC0142 Practical Work

## Overview

**Application:** 01 — Smart Greenhouse (Estufa Inteligente)  
**Protocol:** P4 / PPPP (Pretty Plant-Preserving Protocol)  
**Language:** Rust (edition 2021)  
**Dependencies:** `nom 8.0` (binary parsing), `clap 4.5` (CLI), `file_locking 0.1` (env-file locking)  
**Deadline 2:** 23/06/2026 — source code submission (branch `main`)

> Code style: identifiers are in English; comments and `println!`/`eprintln!` text are in
> Brazilian Portuguese (informal CS, all lowercase). CLI subcommands stay Portuguese
> (`gerenciador`, `sensor`, `atuador`, `cliente`, `completo`) via clap `#[command(name=...)]`,
> even though the underlying enum variants are English.

---

## Project Structure

```
src/
  main.rs                    — CLI (clap): subcommands gerenciador/sensor/atuador/cliente/completo
  protocol/
    mod.rs
    protocol.rs              — Header, message types, encode/decode via nom, Message::new/ack constructors
  components/
    mod.rs
    manager.rs               — TCP server, control logic with hysteresis, watchdog (struct Manager)
    sensor.rs                — TCP client, reads from file, sends SENSOR_DATA every 1s
    actuator.rs              — TCP client, receives ACT_CMD, modifies environment file (struct Actuator)
    client.rs                — TCP client, sends CONFIG + issues SENSOR_QUERY 3x (struct Client)
    devices.rs               — central device registry (single source of truth, see below)
    env_io.rs                — file I/O guarded by an OS advisory lock on a sibling .lock file
    utils.rs                 — connect() retry helper used by sensor/actuator/client
src/env_vars/
  temp.txt / hum.txt / co2.txt   — simulated environment values (float as string)
  temp.lock / hum.lock / co2.lock — lock files used by file_locking (created on first access)
```

---

## Central device registry (`components/devices.rs`)

Single source of truth for all per-device data. Adding a data-only device is one table row.

- `SensorDesc { id, name, file, initial_value, decay }` and `const SENSORS: &[SensorDesc]`
- `ActuatorDesc { id, name, file, variation }` and `const ACTUATORS: &[ActuatorDesc]`
- lookups: `sensor_by_id(id)`, `actuator_by_id(id)`, `name_by_id(id)`

Every component reads from this registry instead of re-declaring file paths, IDs, names, or
rates. The manager keeps its concrete role-based control logic (temperature is the special
multi-actuator 3-state case; humidity/CO2 share the generic `onoff_control` helper), but device
naming, CONNECT validation, env-file init, decay, and component spawning are all registry-driven.

Current IDs: sensors 0=temperatura, 1=umidade, 2=co2; actuators 3=aquecedor, 4=resfriador,
5=irrigador, 6=injetor de co2.

---

## What Is Implemented

| Component | Status | Notes |
|---|---|---|
| P4 Header (8 bytes) | ✅ | magic "PPPP", version, ACK bit, reserved, kind (4 bits), length (16 bits) |
| CONNECT | ✅ | 2 bytes: device kind + unique id |
| SENSOR_DATA | ✅ | sensor_id (1B) + IEEE 754 float — 5 bytes per spec |
| ACT_CMD | ✅ | 1 byte: 0=off, 1=on |
| SENSOR_QUERY | ✅ | sensor_id (1B) — 1 byte per spec |
| SENSOR_RES | ✅ | sensor_id (1B) + float — 5 bytes per spec |
| CONFIG | ✅ | 1B key + 4B IEEE 754 float |
| Manager — accept connections | ✅ | TcpListener, one thread per connection |
| Manager — temperature control | ✅ | 3-state hysteresis (Heating/Cooling/Off) via `temp_control` |
| Manager — humidity control | ✅ | irrigator on below min_hum−hys, off above max_hum+hys (`onoff_control`) |
| Manager — CO2 control | ✅ | injector on below min_co2−hys, off above min_co2+hys (see nota in code) |
| Manager — respond SENSOR_QUERY | ✅ | returns SENSOR_RES with current value |
| Manager — receive CONFIG | ✅ | 8 parameters; config gate only blocks data, not CONNECT/CONFIG |
| Manager — sensor watchdog | ✅ | removes sensors silent >2s, checked every 1.5s |
| Sensor — connect and ACK wait | ✅ | sends CONNECT, waits for ACK (5s timeout) before sending data |
| Sensor — send readings | ✅ | reads file, sends SENSOR_DATA every 1s |
| Actuator — connect and identify | ✅ | sends CONNECT(kind=1, id) |
| Actuator — receive ACT_CMD | ✅ | toggles flag; separate worker thread modifies file |
| Actuator — effect on environment | ✅ | heater +0.5, cooler −0.5, irrigator +1.0, CO2 +2.0 every 500ms |
| Environment decay | ✅ | thread applies each sensor's `decay` (hum 0.3, co2 0.6; temp 0.0) every 1.5s |
| Client — send CONFIG | ✅ | sends 8 parameters at startup (keys 0–6 + 8) |
| Client — SENSOR_QUERY | ✅ | iterates SENSORS registry, 3 rounds |
| `completo` mode | ✅ | starts all components as threads in the same process |

---

## Bugs — All Fixed (session 1)

| Bug | Description | Fix applied |
|-----|-------------|-------------|
| BUG 1 | `sensor_id` was u32 (4B) instead of u8 (1B) | changed field type + encode/decode in SensorData, SensorQuery, SensorRes |
| BUG 2 | deadlock: stream lock held during blocking read while send_act_cmd locked same stream | `try_clone()` splits socket into a dedicated read handle (no lock) and write Arc<Mutex> |
| BUG 3 | sensors did not wait for CONNECT ACK | blocking read with 5s timeout after CONNECT |
| BUG 4 | config gate dropped CONNECT before CONFIG was received | gate only blocks non-CONNECT/non-CONFIG kinds |
| BUG 5 | sensor sleep was 2s instead of 1s | `SLEEP_DURATION_S = 1` |
| BUG 6 | CONFIG param 4 (max_hum) missing from handler | added max_hum field + arm 4 in handle_config |
| MISSING | sensor auto-disconnect watchdog | sensor_last_seen HashMap + watchdog thread |

Known wrinkle (preserved, not fixed): CO2 control uses `min_co2` as both bounds (no `max_co2`
like humidity has). Flagged with a `// nota:` comment in `manager.rs` for a later decision.

---

## Refactoring Applied

### Session 2 — LOC reduction
| Change | Detail |
|--------|--------|
| `Message::new(kind, payload)` | message with auto-filled header — replaced 13 boilerplate blocks |
| `Message::ack(kind)` | ACK message — replaced the old `return_ack` method |
| `Payload::encoded_len()` | payload byte count without encoding — used by the constructors |
| `utils::connect(addr, name)` | retry-loop TCP connect — replaced 5 duplicated loop blocks |
| `apply_decay(file, rate)` | free function for environment decay |
| removed handle_act_cmd / handle_sensor_res stubs | inlined `ActCmd(_) \| SensorRes(_) => None` |

### Session 3 — env-file locking, central registry, English identifiers
| Change | Detail |
|--------|--------|
| `file_locking` crate | replaced the hand-rolled `.lock` spinlock with OS advisory locks (RAII guard, auto-released on process death) |
| `components/devices.rs` | central registry (SENSORS/ACTUATORS tables + lookups); removed `AtuadorTipo` enum, the duplicated `*_FILE` consts across files, `NOMES_DISPOSITIVOS`, and per-device match arms |
| `onoff_control` helper | de-duplicated the identical humidity/CO2 on/off logic |
| English identifiers | all structs/enums/fields/functions/locals/modules renamed to English; files renamed (protocolo→protocol, componentes→components, gerenciador→manager, atuador→actuator, cliente→client, dispositivos→devices). Comments + print text stay Portuguese; CLI commands stay Portuguese via clap. |

---

## Tests

27 unit tests, run with `cargo test`. All passing.

- **protocol** (8): encode/decode round-trips, `encoded_len` vs real encode, ACK has no payload,
  incomplete buffers return None, framing consumes one message at a time, bad magic rejected
- **manager** (10): `onoff_control` hysteresis, `temp_control` state machine, config gate
  (blocks data / always allows CONFIG), `check_config`, full-config readiness, invalid key
- **devices** (4): unique IDs, no sensor/actuator ID overlap, lookups, name resolution
- **env_io** (4): write/read round-trip, init creates file, overwrite, lock-path derivation

---

## Functional Requirements — Status

| Req | Description | Status |
|-----|-------------|--------|
| 1.1 | Sensors have unique IDs | ✅ |
| 1.2 | Sensors connect, identify, wait for ACK | ✅ |
| 1.3 | Sensors send readings every 1s | ✅ |
| 1.4 | Sensors auto-disconnected after 2 missed readings | ✅ |
| 2.1 | Actuators have unique IDs | ✅ |
| 2.2 | Actuators connect and identify themselves | ✅ |
| 2.3 | Actuators can be turned on/off by manager | ✅ |
| 3.1 | Manager accepts sensor/actuator connections | ✅ |
| 3.2 | Manager receives and stores latest readings | ✅ |
| 3.3 | Manager controls actuators via hysteresis | ✅ |
| 3.4 | Manager responds to client queries | ✅ |
| 4   | Client can query latest reading for any sensor | ✅ |

---

## How to Run

```bash
# all-in-one mode (all components in one process):
cargo run -- completo

# if port 8080 is still held by a previous run:
kill $(lsof -ti :8080)

# run the tests:
cargo test

# separate mode (separate OS processes):
cargo run -- gerenciador &
sleep 2
cargo run -- cliente &       # must send CONFIG before sensors/actuators register
sleep 1
cargo run -- sensor --id 0 &   # temperature
cargo run -- sensor --id 1 &   # humidity
cargo run -- sensor --id 2 &   # CO2
cargo run -- atuador --id 3 &  # heater
cargo run -- atuador --id 4 &  # cooler
cargo run -- atuador --id 5 &  # irrigator
cargo run -- atuador --id 6 &  # CO2 injector
```

---

## Development Environment

- OS: Linux (Debian — kernel 6.12 amd64)
- Compiler: Rust stable (rustc + cargo)
- Simulated environment files: `src/env_vars/{temp,hum,co2}.txt`
- File locking: OS advisory lock (`file_locking` crate) held on a sibling `.lock` file while
  reading/writing each env file. The `.lock` files persist on disk as lock handles and are
  invisible to normal reads; the OS releases the lock automatically if a process dies.
