# `werd.yml` reference

`werd.yml` is optional. Put it at the root of a Laravel project to share its setup with your team. Werd reads it when you add the site and never writes or rewrites it.

```yaml
version: 2
php: "8.4"
node: 22
services:
  database: { product: postgresql, version: "18", extensions: [pgvector] }
  cache: redis
  queue: redis@8.2
  mail: true
  storage: rustfs
  search: meilisearch
```

Quote versions with a dot (`"8.4"`, `"8.10"`): unquoted YAML turns `8.10` into the number `8.1`.

## Fields

| Field | Meaning |
| --- | --- |
| `version` | `2`. Files without it are read as version 2; version 1 (Werd 0.1) is still understood. |
| `php` | PHP line the site runs on. Without it the site uses your default PHP. |
| `node` | Node.js major used by `node`, `npm` and `npx` inside the project. `.nvmrc` and `.node-version` work too. |
| `services` | What the site needs, per category. |

## Services

Categories are `database`, `cache`, `queue`, `mail`, `storage` and `search`. Each accepts:

| Form | Example | Meaning |
| --- | --- | --- |
| `true` | `mail: true` | The default product of the category. |
| `false` | `search: false` | Nothing. |
| product | `cache: redis` | That product, any installed version. |
| product@line | `queue: redis@8.2` | That product and version line. |
| object | `database: { product: postgresql, version: "18", extensions: [pgvector] }` | Full form; `extensions` is only for PostgreSQL. |

The default products are:
- `cache` and `queue`: Redis;
- `mail`: Mailpit;
- `storage`: RustFS;
- `search`: Meilisearch.

`database` has no default and needs a product: `postgresql`, `mysql`, `mariadb` or `mongodb`.

## How Werd uses it

When you add the site, Werd links each category to an existing service instance that matches the product, version and extensions. Whatever has no match stays pending. The app shows **Create missing services**, and the CLI offers `werd resolve <site>`. Both create the instances, downloading versions if needed, and link them.

Databases and buckets are named after the site (`My Shop` becomes `my_shop`) and created when the site starts. Copy the `.env` values from the site page, or print them with `werd env <site>`.

## Version 1

Werd 0.1 wrote files like this one. They are translated on the fly:

```yaml
version: 1
php: '8.5'
services:
  postgres: { major: 18, extensions: [pgvector] }
  redis: '7.2'
  mailpit: true
  rustfs: true
```
