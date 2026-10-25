import { Fragment, type ReactNode } from "react";
import { usePhone } from "../lib/phone";

/** One part of a row's detail line; empty parts are left out. */
export type DetailPart = ReactNode;

const shown = (p: DetailPart): boolean => p !== null && p !== undefined && p !== false && p !== "";

/**
 * B-48 O14 B: on phones a data-table row (spells, selection slots, the equipment catalogue, Sources) shows its other columns
 * as one detail line under the name, with the same information; the phone CSS hides those columns. The desktop renders
 * nothing here and keeps its columns (U7).
 */
export function RowDetail({ parts }: { parts: readonly DetailPart[] }) {
  const phone = usePhone();
  const list = parts.filter(shown);
  if (!phone || list.length === 0) return null;
  return <span className="ph-row-detail">{list.map((p, i) => <Fragment key={i}>{i > 0 ? " · " : null}{p}</Fragment>)}</span>;
}
