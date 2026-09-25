# Werd

Werd è un'app desktop open source (MIT) per ambienti Laravel locali. GUI e CLI parlano con lo stesso gestore Rust. Ogni progetto usa processi, porte e directory dati propri, senza Docker o WSL nel flusso Windows.

## Stato attuale

Il prototipo Windows avvia Laravel 13 con PHP 8.5, Caddy HTTPS, PostgreSQL 18 con pgvector, Redis 7.2, Mailpit e RustFS. La GUI permette di aggiungere progetti, installare i runtime, avviarli, fermarli, leggere i log e copiare le variabili `.env`. La CLI espone le stesse operazioni. Due progetti Laravel sono stati provati in parallelo: sito HTTPS, query vettoriale, cache Redis, mail e upload S3 hanno funzionato. Dati PostgreSQL, mail e oggetti sono rimasti dopo arresto e riavvio.

Questa **non è ancora una beta pubblica**. L'installazione di pgvector per Windows richiede ancora Visual Studio Build Tools sul computer dell'utente. Il catalogo dei runtime macOS Apple Silicon, l'audit delle licenze del port Redis e il flusso di aggiornamento degli installer sono ancora da completare. Vedi [fonti e blocchi dei runtime](docs/runtime-sources.md).

## Avvio sviluppo Windows

Prerequisiti: Node.js 22+, Rust stable, Visual Studio 2022 Build Tools con MSVC, WebView2 e Composer per creare un progetto Laravel. Nella root:

```powershell
npm install
cargo build -p werd-core -p werd-cli
npm run tauri -- dev
```

La GUI usa il daemon compilato in `target/debug`. La CLI si trova in `target/debug/werd.exe`. I dati Werd sono in `%LOCALAPPDATA%\Werd`; per test isolati impostare `WERD_HOME` a una directory scelta. Herd, DBngin e Docker non vengono modificati. I siti usano `https://localhost:<porta>` per evitare le porte web già occupate.

## Primo progetto

1. Aggiungi una cartella Laravel dalla GUI o con `werd add C:\percorso\progetto`. Werd crea `werd.yml` se manca.
2. Apri **Diagnostica** e installa PHP, Caddy, PostgreSQL, pgvector, Redis, Mailpit e RustFS. I download hanno versione e SHA-256 fissati. Per pgvector servono Visual Studio Build Tools; è disponibile anche `scripts/build-pgvector-windows.ps1` per la build manuale.
3. Avvia il progetto. Copia le variabili suggerite nella pagina progetto nel suo `.env`, poi esegui le migrazioni Laravel. Werd non modifica `.env` automaticamente.
4. Per rendere attendibile l'HTTPS locale nel browser, usa **Diagnostica → Rendi attendibile**. Questa azione aggiunge la CA generata da Werd alle radici attendibili dell'utente Windows corrente. Caddy non modifica lo store automaticamente.

Per Redis in Laravel installa `predis/predis` e imposta `REDIS_CLIENT=predis`; il runtime PHP iniziale non include l'estensione phpredis. Per S3 installa `league/flysystem-aws-s3-v3`. Crea il bucket indicato da `AWS_BUCKET` prima del primo upload.

```text
werd list
werd add <cartella>
werd up <id>
werd down <id>
werd open <id>
werd logs <id> [werd|postgres|redis|mailpit|rustfs|php|caddy]
werd env <id>
werd runtimes
werd install <php|caddy|postgres|pgvector|redis|mailpit|rustfs>
werd reset-ports <id>
werd trust-ca
werd doctor
```

Le porte restano assegnate al progetto dopo stop e riavvio. Se una porta viene occupata da un altro processo, Werd segnala il conflitto; `reset-ports` o il pulsante nella GUI ne assegna di nuove al prossimo avvio. Aggiorna quindi `.env`.

## Build e struttura

`scripts/package-windows.ps1` prepara i binari CLI e daemon e prova a creare l'installer NSIS. `scripts/package-macos.sh` definisce il packaging Apple Silicon, ancora da verificare su un Mac. I runtime dei servizi non sono inclusi nell'installer: vengono scaricati solo quando richiesti. `crates/werd-core` contiene gestore, catalogo runtime e protocollo RPC; `crates/werd-cli` contiene la CLI; `src-tauri` e `src` contengono la GUI.

Per una prova applicativa completa vedi `scripts/smoke-laravel.php`, da lanciare con il PHP di Werd nella cartella di un progetto Laravel usa e getta dopo aver installato Predis e l'adapter S3.

Il [perimetro di prodotto](docs/product-scope.md) e l'[architettura desktop](docs/desktop-architecture.md) descrivono anche le tappe successive.
