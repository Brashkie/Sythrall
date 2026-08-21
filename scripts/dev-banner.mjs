#!/usr/bin/env node
// Banner de arranque de "npm run dev" — ansimax (https://github.com/Brashkie/ansimax).

import { ascii, gradient } from 'ansimax'

console.log(
  ascii.banner('Sythrall', {
    font: 'small',
    colorFn: (t) => gradient(t, ['#3d9eff', '#b87dff', '#00f5a0']),
  }),
)
console.log(ascii.box('web :5173   api :8420   term :7681   cx :7682', { padding: 1, borderStyle: 'rounded' }))
