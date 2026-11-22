# Seguridad

Para reportar una vulnerabilidad, usa **Security → Report a vulnerability** en la página del repositorio, que es un canal privado. No la publiques en un issue ni adjuntes una prueba que incluya credenciales o datos de otras personas.

Tampoco pegues tokens, contraseñas, archivos de sesión ni claves privadas en issues, pull requests o capturas.

OpenClient está en versión preliminar y el instalador no está firmado. Para comprobar que lo descargado es el archivo original, compara su SHA-256 con el que aparece en las notas del Release:

```powershell
Get-FileHash .\Instalar-OpenClient.exe -Algorithm SHA256
```
