import globals from "globals";
import pluginJs from "@eslint/js";
import pluginReact from "eslint-plugin-react";
import pluginReactHooks from "eslint-plugin-react-hooks";
import pluginUnusedImports from "eslint-plugin-unused-imports";

// =====================================================================
// Lint da aplicação OnyxChat.
//
// Âmbito — a regra que mudou nesta revisão: `src/lib/**` saiu da lista de
// ignorados. A camada `lib` é onde vivem o adaptador de dados
// (`lib/onyx/data-adapter.js`), a formatação de datas e identificadores e
// o contexto de UI. São ficheiros com lógica e sem JSX, que o ESLint
// padrão não produzia nenhum erro por si — daí estar a ser ignorada. É
// exactamente o mesmo argumento que justifica `react/prop-types: off`
// nos componentes: silenciar uma regra porque não se aplica é diferente
// de silenciar uma regra porque não se quer ver o que ela diria.
//
// `src/components/ui/**` continua ignorado: é código do shadcn gerado
// pelo `shadcn add`, não nosso, e regenerado a cada adição de primitivo.
// =====================================================================
export default [
  {
    files: ["src/**/*.{js,mjs,cjs,jsx,ts,tsx}"],
    ignores: ["src/components/ui/**/*"],
    ...pluginJs.configs.recommended,
    ...pluginReact.configs.flat.recommended,
    languageOptions: {
      globals: globals.browser,
      parserOptions: {
        ecmaVersion: 2022,
        sourceType: "module",
        ecmaFeatures: {
          jsx: true,
        },
      },
    },
    settings: {
      react: {
        version: "detect",
      },
    },
    plugins: {
      react: pluginReact,
      "react-hooks": pluginReactHooks,
      "unused-imports": pluginUnusedImports,
    },
    rules: {
      "no-unused-vars": "off",
      "react/jsx-uses-vars": "error",
      "react/jsx-uses-react": "error",
      "unused-imports/no-unused-imports": "error",
      "unused-imports/no-unused-vars": [
        "warn",
        {
          vars: "all",
          varsIgnorePattern: "^_",
          args: "after-used",
          argsIgnorePattern: "^_",
        },
      ],
      "react/prop-types": "off",
      "react/react-in-jsx-scope": "off",
      "react/no-unknown-property": [
        "error",
        { ignore: ["cmdk-input-wrapper", "toast-close"] },
      ],
      // `exhaustive-deps` fica a `warn` e não a `error`. A regra não conhece
      // o contrato de `useDetails(node, deps)`, que recebe um nó React como
      // dependência para o publicar no contexto: um nó não é um valor
      // comparável, e a regra não tem como saber que a intenção é a de
      // republicar em cada render. A `error` transformaria isso num erro de
      // compilação permanente, e a correcção seria silenciar a regra em cada
      // ponto — exactamente o que esta revisão evita.
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "warn",
    },
  },
];