# App desktop e architettura iniziale

## Scelta dello stack

La scelta iniziale è **Tauri 2** per la shell desktop, con frontend TypeScript e backend Rust. Tauri usa la WebView del sistema operativo: WebView2 su Windows e WebKit su macOS. Electron resta un'alternativa se il prototipo rivela problemi concreti di compatibilità o di esperienza. La scelta evita di distribuire una copia di Chromium dentro l'app, ma non elimina il costo dei servizi avviati dall'utente.

Il gestore dei servizi deve vivere in un processo separato dalla finestra: chiudere la dashboard non deve spegnere accidentalmente database o worker. GUI e CLI comunicano con lo stesso gestore tramite un'API locale. Il gestore possiede lo stato dei progetti, controlla i processi, raccoglie log ed esegue verifiche di salute. Le operazioni privilegiate, come installare una CA locale, sono isolate e richieste solo quando servono.

## Schermate obbligatorie

| Schermata | Azioni principali |
| --- | --- |
| Dashboard | Vedere progetti attivi, servizi condivisi, porte occupate e problemi da risolvere; avviare o fermare un progetto. |
| Progetti | Aggiungere una cartella, rilevare Laravel, scegliere PHP e dominio, aprire sito e terminale. |
| Dettaglio progetto | Scegliere dipendenze, avviare o fermare worker/scheduler/Reverb, leggere `.env` suggerito e stato dei servizi. |
| Servizi | Installare versioni, configurare e controllare PostgreSQL/pgvector, MySQL/MariaDB, Redis, Mailpit, RustFS e gli altri servizi supportati. |
| Database e storage | Creare database, utenti e bucket; eseguire backup e ripristino. |
| Diagnostica | Leggere log, mail e dump; vedere errori, porte, processi e azioni di riparazione. |
| Impostazioni | Gestire PHP, Node, certificati, percorsi, avvio automatico e aggiornamenti. |

La GUI deve mostrare sempre lo stato reale del gestore, non solo l'ultimo comando inviato. Errori e operazioni lunghe devono avere progresso e una via di recupero visibile.

## Primo rilascio utilizzabile

Il flusso minimo completo comprende onboarding, aggiunta di un progetto Laravel, selezione PHP, dominio HTTPS, avvio e arresto del progetto, PostgreSQL con pgvector, Redis, Mailpit e RustFS, stato e log. Tutto è azionabile dall'app desktop. La CLI viene sviluppata contro la stessa API, senza logica separata.

## Verifiche prima di confermare lo stack

- Avvio e consumo di risorse della GUI a riposo su Windows e macOS.
- Gestione di finestre, tray, notifiche e aggiornamenti su entrambe le piattaforme.
- Esecuzione sicura e supervisione dei processi anche a finestra chiusa.
- Distribuzione, firma e aggiornamento degli installer.

Riferimenti: [Tauri](https://tauri.app/start/), [prerequisiti Tauri](https://tauri.app/start/prerequisites/), [Electron](https://www.electronjs.org/docs/latest).
