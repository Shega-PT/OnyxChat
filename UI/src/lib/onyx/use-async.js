// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnyxChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

import { useCallback, useEffect, useRef, useState } from 'react';

/** Carrega dados do adaptador, expondo estado de carregamento, erro e recarregamento. */
export function useAsync(loader, deps = []) {
  const [state, setState] = useState({ loading: true, data: null, error: null });
  const loaderRef = useRef(loader);
  loaderRef.current = loader;

  const run = useCallback(() => {
    let active = true;
    setState((previous) => ({ ...previous, loading: true, error: null }));
    Promise.resolve()
      .then(() => loaderRef.current())
      .then((data) => {
        if (active) setState({ loading: false, data, error: null });
      })
      .catch((error) => {
        if (active) setState({ loading: false, data: null, error });
      });
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => run(), [run, ...deps]);

  return { ...state, reload: run };
}