import type { Category, ServiceInstance } from "./api";

export const CATEGORIES: Category[] = ["database", "cache", "queue", "mail", "storage", "search"];

/** Products that can serve each site category. */
export const CATEGORY_PRODUCTS: Record<Category, string[]> = {
  database: ["postgresql", "mysql", "mariadb", "mongodb"],
  cache: ["redis"],
  queue: ["redis"],
  mail: ["mailpit"],
  storage: ["rustfs"],
  search: ["meilisearch"],
};

/** Instances a site can link for a category. */
export function instancesFor(category: Category, instances: ServiceInstance[]): ServiceInstance[] {
  return instances.filter((instance) => CATEGORY_PRODUCTS[category].includes(instance.product));
}

/** Log sources a site has; services have their own log. */
export const SITE_LOG_SOURCES = ["werd", "php", "caddy"] as const;
