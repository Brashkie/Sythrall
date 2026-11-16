//! La capa de servicio del sidecar: escanear un proyecto, declarar
//! capacidades, armar el shape que el backend relaya, los scores de
//! salud, el historial de snapshots, el log persistente y el motor de CSS.

pub mod analisis;
pub mod apis;
pub mod cssatomico;
pub mod cssusage;
pub mod diagrama;
pub mod editor;
pub mod grafo;
pub mod historial;
pub mod logs;
pub mod logstore;
pub mod metricas;
pub mod ml;
pub mod mlpanel;
pub mod plugin;
pub mod proyecto;
pub mod rich;
pub mod salud;
pub mod subidas;
pub mod zip;
pub mod scanner;
pub mod sistema;
