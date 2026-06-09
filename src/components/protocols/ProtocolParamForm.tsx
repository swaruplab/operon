import { useEffect, useState, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Loader2, Play, Settings2 } from 'lucide-react';

interface ProtocolParam {
  name: string;
  default: string;
  kind: 'path' | 'integer' | 'number' | 'boolean' | 'string';
  template_file: string;
}

interface ProtocolParamFormProps {
  slug: string;
  onRun: (paramsByVar: Record<string, string>) => void;
}

export function ProtocolParamForm({ slug, onRun }: ProtocolParamFormProps) {
  const [params, setParams] = useState<ProtocolParam[] | null>(null);
  const [values, setValues] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    invoke<ProtocolParam[]>('get_protocol_template_params', { slug })
      .then((result) => {
        if (cancelled) return;
        setParams(result);
        const initial: Record<string, string> = {};
        for (const p of result) initial[p.name] = p.default;
        setValues(initial);
      })
      .catch((e) => {
        if (cancelled) return;
        setError(String(e));
        setParams([]);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [slug]);

  const groupedByTemplate = useMemo(() => {
    const groups: Record<string, ProtocolParam[]> = {};
    for (const p of params || []) {
      if (!groups[p.template_file]) groups[p.template_file] = [];
      groups[p.template_file].push(p);
    }
    return groups;
  }, [params]);

  const handleChange = (name: string, val: string) => {
    setValues((prev) => ({ ...prev, [name]: val }));
  };

  if (loading) {
    return (
      <div className="flex items-center gap-2 px-3 py-4 text-secondary">
        <Loader2 className="w-3.5 h-3.5 animate-spin" />
        <span className="text-[11px]">Loading parameters…</span>
      </div>
    );
  }

  if (error) {
    return (
      <div className="px-3 py-2 bg-red-950/20 border border-red-900/30 rounded">
        <p className="text-[10px] text-red-700 dark:text-red-300">{error}</p>
      </div>
    );
  }

  if (!params || params.length === 0) {
    return (
      <div className="px-3 py-3 bg-amber-950/20 border border-amber-900/30 rounded">
        <p className="text-[11px] text-amber-700 dark:text-amber-300">
          Auto-config form not yet supported for this template type — edit the script directly.
        </p>
      </div>
    );
  }

  const firstTemplate = Object.keys(groupedByTemplate)[0];

  return (
    <div className="space-y-3">
      {Object.entries(groupedByTemplate).map(([template, items]) => (
        <div key={template} className="space-y-2">
          {Object.keys(groupedByTemplate).length > 1 && (
            <div className="flex items-center gap-1.5 px-1">
              <Settings2 className="w-3 h-3 text-teal-600 dark:text-teal-400" />
              <span className="text-[10px] font-semibold uppercase tracking-wider text-secondary">
                {template}
              </span>
            </div>
          )}
          <div className="space-y-2">
            {items.map((p) => (
              <ParamRow
                key={`${template}:${p.name}`}
                param={p}
                value={values[p.name] ?? ''}
                onChange={(v) => handleChange(p.name, v)}
              />
            ))}
          </div>
        </div>
      ))}

      <button
        onClick={() => onRun({ ...values, __template__: firstTemplate || '' })}
        className="w-full flex items-center justify-center gap-1.5 px-3 py-2 bg-teal-600 hover:bg-teal-500 text-white rounded-lg text-xs font-medium transition-colors"
      >
        <Play className="w-3 h-3" />
        Run with these parameters
      </button>
    </div>
  );
}

interface ParamRowProps {
  param: ProtocolParam;
  value: string;
  onChange: (v: string) => void;
}

function ParamRow({ param, value, onChange }: ParamRowProps) {
  const labelEl = (
    <label
      htmlFor={`param-${param.name}`}
      className="text-[10px] text-muted font-medium uppercase tracking-wider block mb-1"
    >
      {param.name}
      <span className="ml-1 normal-case text-[9px] text-subtle">({param.kind})</span>
    </label>
  );

  if (param.kind === 'boolean') {
    const checked =
      value === 'true' ||
      value === 'True' ||
      value === 'yes' ||
      value === 'YES' ||
      value === '1';
    return (
      <div>
        {labelEl}
        <label className="flex items-center gap-2 cursor-pointer">
          <input
            id={`param-${param.name}`}
            type="checkbox"
            checked={checked}
            onChange={(e) => onChange(e.target.checked ? 'true' : 'false')}
            className="w-3.5 h-3.5 accent-teal-600"
          />
          <span className="text-[11px] text-secondary">{checked ? 'true' : 'false'}</span>
        </label>
      </div>
    );
  }

  if (param.kind === 'path') {
    return (
      <div>
        {labelEl}
        <div className="flex gap-1">
          <input
            id={`param-${param.name}`}
            type="text"
            value={value}
            onChange={(e) => onChange(e.target.value)}
            placeholder={param.default || '/path/to/...'}
            className="flex-1 bg-panel border border-border-strong rounded px-2 py-1 text-[11px] text-primary placeholder:text-subtle outline-none focus:border-teal-600 font-mono"
          />
        </div>
      </div>
    );
  }

  if (param.kind === 'integer' || param.kind === 'number') {
    return (
      <div>
        {labelEl}
        <input
          id={`param-${param.name}`}
          type="number"
          step={param.kind === 'integer' ? '1' : 'any'}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          className="w-full bg-panel border border-border-strong rounded px-2 py-1 text-[11px] text-primary outline-none focus:border-teal-600 font-mono"
        />
      </div>
    );
  }

  return (
    <div>
      {labelEl}
      <input
        id={`param-${param.name}`}
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={param.default}
        className="w-full bg-panel border border-border-strong rounded px-2 py-1 text-[11px] text-primary placeholder:text-subtle outline-none focus:border-teal-600 font-mono"
      />
    </div>
  );
}
