# Fixtures canônicas do protocolo v3

Estes arquivos são consumidos pelos testes Rust e Kotlin. Os arquivos `.hex` usam bytes separados por espaço e podem conter quebras de linha; o parser existe somente no código de teste. Os JSON são payloads UTF-8 compactos, sem tokens ou certificados reais.

- `hello.json`: payload HELLO v3.
- `config.json`: payload CONFIG.
- `input-touch-pen.hex`: três contatos (toque, toque e caneta) no formato INPUT.
- `scroll.hex`: payload SCROLL com quatro `f32` big-endian.

Alterar um fixture exige atualizar os testes dos dois clientes e registrar a decisão no relatório da tarefa.
