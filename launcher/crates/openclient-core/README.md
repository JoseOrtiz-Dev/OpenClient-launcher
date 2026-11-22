# Motor compartido

Biblioteca Rust sin dependencias de Tauri, Qt o React. No se publica en crates.io.

`performance` contiene los tipos de hardware, la detección de RAM y la construcción
de argumentos JVM que ya usaba el launcher. Tauri conserva adaptadores con los
mismos comandos para mantener la compatibilidad con la interfaz actual.

Desde la raíz:

```powershell
cargo test --offline --manifest-path src-tauri/Cargo.toml -p openclient-core
```

La interfaz Qt consulta hardware mediante `../openclient-ffi`, una ABI C versionada
que devuelve JSON y libera la memoria en Rust. No exponer estructuras Rust directamente
a C++ ni transportar tokens a QML. El resto de los servicios aún está pendiente
de extracción; no existe paridad funcional con Tauri.
