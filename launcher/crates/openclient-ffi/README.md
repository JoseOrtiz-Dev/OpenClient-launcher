# Puente Rust / C++

Biblioteca dinámica Windows x64 con ABI versión 3.

| Símbolo C | Resultado |
| --- | --- |
| `uint32_t openclient_abi_version(void)` | Versión del contrato; C++ rechaza versiones desconocidas |
| `char *openclient_hardware_json(void)` | JSON UTF-8 terminado en NUL, o NULL si falla |
| `char *openclient_request_json(const char *, size_t)` | Operación de biblioteca, perfil local o juego en JSON UTF-8, limitada a 8192 bytes |
| `void openclient_free_string(char *)` | Libera una respuesta de esta misma biblioteca; acepta NULL |

La respuesta de hardware usa `{ "ok": true, "abi_version": 3, "hardware": { ... } }`, con
los campos públicos de `openclient-core::performance::HardwareProfile`.

Rust asigna la memoria y Rust la libera. C++ copia el JSON antes de liberar la
respuesta; no guarda el puntero ni descarga la DLL antes de terminar. No usar
`delete`, `free`, punteros desplazados ni doble liberación. Los panics capturables
se contienen en Rust y se devuelve NULL; un fallo de asignación fatal puede abortar
el proceso, por lo que esto no es un límite de aislamiento frente a fallos.

El controlador Qt envía la ruta de su propio directorio de datos en las
operaciones de biblioteca y adquiere un bloqueo para cada lectura o escritura.
Las operaciones admitidas son listar, crear, editar y retirar instancias, además
de validar un perfil local. La respuesta devuelve `result` o `error`.
La consulta de hardware se ejecuta en un trabajador de QtConcurrent.
El inicio del juego también se ejecuta en un trabajador. El controlador consulta
`game_status` y mantiene el bloqueo de la biblioteca hasta el cierre de Java.
La DLL permanece cargada porque el motor espera el cierre en un hilo de Rust.

Compilación y pruebas desde la raíz:

```powershell
cargo test --locked --manifest-path crates/openclient-ffi/Cargo.toml --target-dir build/rust
npm.cmd run qt:build -- -SmokeTest
```

El lockfile del puente se versiona porque genera un binario distribuible. Cambios
incompatibles requieren subir la versión de ABI y adaptar los consumidores.
