# Third-party notices

## Fig Autocomplete Specs

FileTerm uses command completion specifications from [withfig/autocomplete](https://github.com/withfig/autocomplete), published as `@withfig/autocomplete` version **2.692.3**. Thanks to Fig and the community contributors.

The original specification data is converted to offline declarative JSON by `apps/tauri/scripts/sync-completion-specs.mjs`. FileTerm supplies its own input tracking, context resolver, candidate UI and SSH file-channel integration. Generator scripts and executable callbacks are not included in the generated data.

The upstream package metadata says ISC, while both its repository and distributed `LICENSE` contain the following MIT license. FileTerm preserves the actual distributed copyright and license below, also bundled in the app and visible under Settings → System → Third-party open source.

```text
MIT License

Copyright (c) 2021 Hercules Labs Inc. (Fig)

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
