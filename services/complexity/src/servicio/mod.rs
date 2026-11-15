//! La capa de servicio del sidecar: escanear un proyecto, declarar
//! capacidades, armar el shape que el backend relaya, los scores de
//! salud, el log persistente y el motor de CSS.

pub mod cssatomico;
pub mod cssusage;
pub mod logstore;
pub mod ml;
pub mod plugin;
pub mod rich;
pub mod salud;
pub mod scanner;
