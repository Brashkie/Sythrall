//! Infraestructura compartida de arbol: recorrerlo, leer nodos, contar
//! lineas, extraer simbolos y estructura. Lo que esta aca no sabe de
//! ningun lenguaje en particular — y las cinco copias de `walk` que la
//! Fase 32 tuvo que cazar salieron justamente de no tener este lugar.

pub mod parser;
pub mod raw;
pub mod structure;
pub mod symbols;
pub mod tsnode;
pub mod walk;
