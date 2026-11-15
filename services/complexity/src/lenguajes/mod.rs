//! El front-end del motor: un modulo por lenguaje, cada uno con su
//! gramatica y su extension. Es la frontera donde el texto se vuelve
//! arbol; todo lo que sigue trabaja sobre arboles, no sobre fuentes.

pub mod adaparse;
pub mod asmparse;
pub mod binparse;
pub mod cparse;
pub mod cssparse;
pub mod dparse;
pub mod fparse;
pub mod htmlparse;
pub mod jsts;
pub mod manifest;
pub mod notebook;
pub mod rsparse;
