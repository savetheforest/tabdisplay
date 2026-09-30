# Política de privacidade do TabDisplay

Última atualização: 26/09/2026

O TabDisplay transforma o seu tablet Android em segundo monitor do seu PC (Windows ou Mac). Ele funciona
**localmente**: a imagem, o áudio, o toque e a caneta vão direto do PC para o tablet pela sua rede ou pelo
cabo USB, sem passar por nenhum servidor nosso.

## O que coletamos
**Nada**, com a configuração padrão. Não temos contas, anúncios, análises nem rastreadores, e não vendemos
nem compartilhamos dados.

## O que fica guardado no tablet
Só o necessário para reconectar, em armazenamento privado do app (apagado ao desinstalar):
- um identificador aleatório do tablet e o nome do aparelho, enviados ao PC ao conectar;
- por PC já pareado, o código de pareamento (token) e a impressão digital (SHA-256) do certificado do PC;
- preferências do app (mostrar estatísticas, silenciar áudio, último endereço IP usado).

## Como os dados trafegam
Toda a sessão entre o PC e o tablet (imagem, áudio, toques, token) é criptografada com TLS 1.3. O tablet
memoriza o certificado de cada PC na primeira conexão e recusa um certificado diferente depois.

## Permissões que o app pede
- **Internet / rede**: conectar ao seu PC (Wi‑Fi, Ethernet ou USB).
- **Multicast do Wi‑Fi**: achar o PC automaticamente na rede local.
- **Serviço em primeiro plano, WakeLock e notificações**: manter a sessão ligada com a tela bloqueada ou
  com outro app na frente; a notificação mostra que há uma sessão ativa.

O app não acessa contatos, localização, microfone, câmera, fotos, arquivos nem outras contas.

## Relatórios de falhas (opcional)
Versões que incluem o relatório de falhas (Sentry) enviam, quando o app trava, o registro técnico do erro
(tipo do erro, pilha de chamadas, versão do app e do Android) e uma trilha de etapas da conexão (por exemplo
“conectou”, “pareou”). Não vão IP, nomes de PC ou de tablet, códigos nem tokens. Sem essa configuração
nada é enviado.

## Crianças
O app não é direcionado a crianças e não coleta dados de ninguém.

## Mudanças
Se esta política mudar, a nova versão aparece neste endereço com a data atualizada.

## Contato
Este documento ainda não está pronto para publicação: falta substituir o contato abaixo por um endereço real
do responsável pela distribuição. Não publicar com o placeholder.

Dúvidas: [SEU E-MAIL DE CONTATO]
