# Código de terceros dentro de los binarios

Sythrall se distribuye bajo GPL-3.0-or-later (ver [`LICENSE`](LICENSE)), pero
sus binarios llevan compilado adentro código **C y assembly de otros
proyectos**. Las licencias de ese código piden reproducir su aviso de
copyright cuando se distribuye el binario, y este archivo existe para eso.

**Por qué hace falta un archivo aparte.** Las dependencias de Rust que son Rust
puro traen su licencia dentro de su propio paquete, así que ahí no hay nada que
hacer. El problema son las que arrastran un compilador de C (`cc`) y meten
código ajeno en el ejecutable: ese código viaja en el binario y su licencia
viaja con él, pero **la mayoría de estos paquetes no publica su archivo de
licencia en crates.io** — se comprobó uno por uno: de las nueve gramáticas de
tree-sitter, **ninguna** incluye el aviso en el paquete. Sin este archivo, el
aviso simplemente no llegaría a quien recibe el binario.

Se listan sólo las dependencias con código compilado adentro. Se determinó
mecánicamente, no de memoria: son las que dependen de `cc` en
`cargo tree -i cc`.

---

## tree-sitter 0.25.10 y sus nueve gramáticas

El parser incremental con el que Sythrall lee C, C++, JavaScript, TypeScript,
Rust, Fortran, Ada y D. La biblioteca y cada gramática se compilan desde C
—generado por tree-sitter— dentro de `complexity-engine`.

**Todas son MIT**, y el texto de la licencia es el mismo para todas; lo que
cambia es el titular y el año. Los avisos, verificados contra el `LICENSE` de
cada repositorio:

| Paquete | Aviso de copyright | Repositorio |
|---|---|---|
| `tree-sitter` 0.25.10 | `Copyright (c) 2018 Max Brunsfeld` | <https://github.com/tree-sitter/tree-sitter> |
| `tree-sitter-c` 0.23.4 | `Copyright (c) 2014 Max Brunsfeld` | <https://github.com/tree-sitter/tree-sitter-c> |
| `tree-sitter-cpp` 0.23.4 | `Copyright (c) 2014 Max Brunsfeld` | <https://github.com/tree-sitter/tree-sitter-cpp> |
| `tree-sitter-javascript` 0.23.1 | `Copyright (c) 2014 Max Brunsfeld` | <https://github.com/tree-sitter/tree-sitter-javascript> |
| `tree-sitter-typescript` 0.23.2 | `Copyright (c) 2017 Max Brunsfeld` | <https://github.com/tree-sitter/tree-sitter-typescript> |
| `tree-sitter-rust` 0.23.3 | `Copyright (c) 2017 Maxim Sokolov` | <https://github.com/tree-sitter/tree-sitter-rust> |
| `tree-sitter-ada` 0.1.0 | `Copyright (c) 2023 Emmanuel Briot` | <https://github.com/briot/tree-sitter-ada> |
| `tree-sitter-d` 0.8.2 | `Copyright 2024 Garrett D'Amore` (su código trae también avisos de 2023) | <https://github.com/gdamore/tree-sitter-d> |
| `tree-sitter-fortran` 0.6.0 | **ninguno** — ver la nota de abajo | <https://github.com/stadelmanma/tree-sitter-fortran> |

**Sobre `tree-sitter-fortran`:** su archivo de licencia dice "The MIT License
(MIT)" y pasa directo a "Permission is hereby granted…", **sin ninguna línea de
copyright**. Se verificó el archivo completo. Acá no se inventa un titular: la
licencia es MIT y el aviso que su autor eligió no poner no se pone en su
nombre.

Texto de la licencia MIT, que aplica a los nueve paquetes de la tabla con el
aviso de copyright que corresponda a cada uno:

```text
MIT License

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

---

## ring 0.17.14

La criptografía de TLS: entra por `rustls`, que es el cliente HTTPS con el que
`/analyze/api` chequea APIs externas. `ring` compila **C y assembly tomados de
BoringSSL** dentro del binario, así que su licencia no es una sola.

<https://github.com/briansmith/ring>

El propio proyecto lo declara así: el código nuevo es ISC, y el que viene de
BoringSSL está bajo Apache 2.0 (y algo bajo ISC), con el aviso en la cabecera
de cada archivo.

**ISC — el código propio de `ring`:**

```text
Copyright 2015-2025 Brian Smith.

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY
SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION
OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN
CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
```

**Apache 2.0 — el código que viene de BoringSSL.** La Apache 2.0 pide entregar
una copia de la licencia a quien recibe el trabajo, y son 272 líneas, así que
el texto íntegro está en [`licenses/Apache-2.0.txt`](licenses/Apache-2.0.txt)
en vez de acá.

`ring` incluye además código del proyecto `once_cell`, bajo Apache 2.0 y MIT
(los dos textos están en el propio paquete, en `src/polyfill/once_cell/`).

---

## Lo que NO está en esta lista, y por qué

- **Las dependencias de Rust que son Rust puro** traen su licencia dentro de su
  propio paquete, que se distribuye con el código fuente. No hay nada que
  reproducir acá. `flate2`, por ejemplo, usa `miniz_oxide` —una
  reimplementación de DEFLATE en Rust— y **no** compila zlib en C, cosa que se
  verificó mirando su árbol de dependencias.
- **Los kernels de C, Zig, Fortran y Assembly de `services/complexity/src/bench/`**
  son código propio de Sythrall, escrito para este proyecto. Están bajo la
  misma GPL-3.0 que el resto.
- **Las dependencias de npm** son de desarrollo o viajan en el bundle del
  frontend con su propia licencia declarada en `package-lock.json`. Monaco
  Editor, mermaid, chart.js y xterm son MIT; cada paquete trae su aviso.
