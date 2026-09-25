# Perimetro del prodotto

## Riferimento funzionale

Il traguardo è coprire le funzioni pubblicamente documentate di Laravel Herd per Windows e macOS, comprese quelle Pro, attraverso un'implementazione indipendente. Le funzioni e la loro priorità verranno verificate con casi d'uso reali, non solo con una somiglianza visiva.

| Area | Comportamento richiesto |
| --- | --- |
| Siti | Collegare un progetto o parcheggiare una directory; rilevare la document root Laravel; assegnare un dominio `.test`; aprire il sito nel browser. |
| Web e TLS | Servire più siti contemporaneamente, instradare ogni sito al PHP scelto e installare certificati locali attendibili. |
| PHP | Installare e aggiornare più versioni; selezionare una versione per sito e per CLI; gestire le estensioni necessarie. |
| Strumenti | Rendere disponibili Composer, Laravel installer e versioni selezionabili di Node.js. |
| Servizi | Installare, configurare, avviare, fermare e aggiornare PostgreSQL, MySQL/MariaDB, Redis, Mailpit, storage S3 locale, Typesense, Meilisearch e Reverb. |
| Database | Creare database e utenti, conservare i dati, effettuare backup e ripristino; supportare PostgreSQL con pgvector come caso esplicito. |
| Mail | Catturare la posta in uscita e mostrarla per progetto. |
| Diagnostica | Visualizzare log PHP e Laravel, intercettare `dump()`/`dd()`, gestire Xdebug e mostrare lo stato dei processi. |
| Progetti | Salvare una configurazione versionabile per progetto; avviare e fermare solo le dipendenze richieste; gestire collisioni di porte e risorse condivise. |
| Interfacce | Offrire un'app desktop completa con dashboard, dettagli progetto, catalogo servizi, posta, log e impostazioni. La CLI espone le stesse operazioni per automazione. |

L'integrazione con servizi esterni, come Forge o strumenti di condivisione pubblica, richiede una valutazione separata delle API disponibili. Non deve impedire la parità delle funzioni locali.

## Primo caso d'uso completo

Un progetto Laravel su Windows e uno su macOS dichiarano PHP, PostgreSQL con pgvector, Redis, Mailpit e RustFS. L'utente può svolgere l'intero flusso dalla GUI:

1. Collegare il repository senza modificare a mano la configurazione di sistema.
2. Avviare il sito e i servizi richiesti con una sola azione.
3. Accedere al sito tramite HTTPS locale.
4. Vedere stato, log, posta e connessioni dalla GUI o dalla CLI.
5. Fermare il progetto senza fermare servizi ancora usati da altri progetti.
6. Riprendere il lavoro senza perdere database o oggetti salvati.

Le stesse operazioni essenziali sono disponibili nella CLI. Il primo rilascio utilizzabile include entrambe le interfacce.

Questo caso d'uso è una prima tappa, non il limite del prodotto: la parità con le altre funzioni elencate resta l'obiettivo.

## Decisioni tecniche da validare con un prototipo

- Distribuzione di PHP e delle sue estensioni su entrambe le piattaforme.
- Gestione di DNS, porte privilegiate e certificati locali senza interventi ripetuti dell'amministratore.
- Abbinamento affidabile fra versioni di PostgreSQL e pgvector su Windows e macOS.
- Formato della configurazione di progetto e compatibilità fra versioni.
- Strategia di aggiornamento e migrazione dei dati dei servizi.
- Eventuale motore per container opzionale e suo costo in memoria quando è acceso.

## Fonti consultate

- [Laravel Herd per Windows](https://herd.laravel.com/windows)
- [Laravel Herd per macOS](https://herd.laravel.com/)
- [pgvector](https://github.com/pgvector/pgvector)
- [RustFS](https://docs.rustfs.com/en/installation)
