//! Analisis de flujo: el grafo de control, SSA, dominadores, el grafo de
//! llamadas, intervalos, y lo que se deduce de recorrerlos (asignaciones
//! muertas, recursion, pureza, precondiciones de control).

pub mod cfg;
pub mod control;
pub mod graph;
pub mod intervals;
pub mod muertas;
pub mod purity;
pub mod recursion;
pub mod ssa;
