import type { KlvFieldWithChange } from '../types';

interface Props {
  fields: KlvFieldWithChange[];
  updatedAt: string | null;
}

export function MetadataPanel({ fields, updatedAt }: Props) {
  return (
    <div className="metadata-panel">
      <h3>KLV Metadata</h3>
      {fields.length === 0 ? (
        <div className="metadata-empty">No metadata received</div>
      ) : (
        <>
          {fields.map((f) => (
            <div key={f.tag} className={`metadata-row${f.changed ? ' changed' : ''}`}>
              <span className="metadata-name" title={f.name}>
                {f.name}
              </span>
              <span className="metadata-value">
                {f.value}
                {f.unit && <span className="metadata-unit">{f.unit}</span>}
              </span>
            </div>
          ))}
          {updatedAt && <div className="metadata-updated">Updated {updatedAt}</div>}
        </>
      )}
    </div>
  );
}
