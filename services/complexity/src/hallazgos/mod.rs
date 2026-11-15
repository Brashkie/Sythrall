//! Las reglas que emiten findings. Todas producen `StructuralSmell` o un
//! tipo de hallazgo propio, y todas pasan por `decidibilidad.rs` al
//! serializar: lo que esta aca afirma algo sobre el codigo del usuario.

pub mod classifiers;
pub mod cripto;
pub mod csecurity;
pub mod datastructures;
pub mod falsesharing;
pub mod jsts_security;
pub mod locality;
pub mod memlayout;
pub mod modernization;
pub mod naming;
pub mod pymodern;
pub mod security;
pub mod sfinae;
pub mod smells;
pub mod tipos;
pub mod wasm;
