# 🤝 Contributing to TrayDrop

Thanks for taking the time to contribute! Contributions of every kind —
bug reports, ideas, docs, code — are welcome. ⚡

## How to report bugs / request features

Use the [issue tracker](https://github.com/yourname/traydrop/issues) and include:

* your OS + Python version (`python --version`),
* exact steps to reproduce,
* expected vs. actual behaviour,
* tray/console output if relevant.

## Development workflow

```bash
git clone https://github.com/yourname/traydrop.git && cd traydrop
python -m venv .venv && source .venv/bin/activate    # Windows: .venv\Scripts\activate
pip install -e .
pip install pytest
pytest tests -q
```

1. Create a branch: `git checkout -b feature/short-description`
2. Keep commits focused; use imperative subjects
   (`add tray quit handler`, not `added stuff`).
3. Run the test suite before pushing.
4. Open a pull request against `main` — fill in the template, link issues
   (`Fixes #123`).

## Style guidelines

* Follow [PEP 8](https://peps.python.org/pep-0008/); docstrings on public APIs.
* Type hints where practical (`from __future__ import annotations` is already used).
* Platform-specific code stays inside `traydrop.window` / guarded by
  `sys.platform` checks — keep the rest portable.
* No new runtime dependencies without a strong reason (the tray icon is drawn
  with Pillow precisely to avoid shipping binaries).

## What I can help with?

Look for issues tagged [`good first issue`](https://github.com/yourname/traydrop/labels/good%20first%20issue) 🏷️.

By participating you agree to the [Code of Conduct](CODE_OF_CONDUCT.md).
