import { useState } from "react";
import { RuntimeTable } from "../components/RuntimeTable";
import { useT } from "../i18n";
import type { Runtimes } from "../runtimes";
import { PageHeader, Section } from "../ui";

export function Node({ runtimes }: { runtimes: Runtimes }) {
  const t = useT();
  const [onlyLts, setOnlyLts] = useState(true);
  const rows = runtimes.rows.filter((row) => row.product === "node" && (!onlyLts || row.lts || row.installed !== null));

  return (
    <>
      <PageHeader title={t.node.title} />
      <div className="page-body">
        <Section
          title={t.node.versions}
          description={t.node.versionsHint}
          action={
            <label className="checkbox">
              <input type="checkbox" checked={onlyLts} onChange={(event) => setOnlyLts(event.target.checked)} />
              {t.node.onlyLts}
            </label>
          }
        >
          <RuntimeTable rows={rows} runtimes={runtimes} showDefault showLts />
        </Section>
      </div>
    </>
  );
}
