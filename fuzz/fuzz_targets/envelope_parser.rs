// Required Notice: ShegaPT / Carlos Alberto da Silva Brito — PolyForm Noncommercial 1.0.0 <https://github.com/Shega-PT/OnixChat>
//
// Licença base do projecto: PolyForm Noncommercial 1.0.0.
// Não é open source pela definição da OSI. Uso comercial
// exige autorização do titular — ver COMMERCIAL-LICENSE.md.

// =====================================================================
// envelope_parser — fuzz do parser do envelope binário
// ---------------------------------------------------------------------
// Fronteira: qualquer byte chegado por transporte P2P/IPC é parseado
// por `Envelope::abrir` (docs/message_format.md §Regras de parsing).
//
// Propriedade verificada além da não-crash: **round-trip identidade**.
// Se o parser aceita um envelope, re-serializá-lo tem de devolver
// exactamente os mesmos bytes — um parser que "aceite" mas altere um
// campo estaria a corromper a região assinada a jusante.
// =====================================================================

#![no_main]

use libfuzzer_sys::fuzz_target;

use onyxchatd::envelope::Envelope;

fuzz_target!(|dados: &[u8]| {
    match Envelope::abrir(dados) {
        // Rejeição: é o caminho previsto para a esmagadora maioria dos
        // inputs — nunca pode ser um crash (o próprio erro já é seguro).
        Err(_) => {}
        Ok((env, assinatura)) => {
            // Aceitou → os campos têm de reproduzir os bytes originais.
            // Se isto falhar, o parser engoliu ou inventou bytes.
            assert_eq!(
                env.fechar(&assinatura),
                dados,
                "o envelope re-serializado difere dos bytes aceites"
            );
        }
    }
});
