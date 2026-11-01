# Reelhouse (Qt) — checklist geral (auditoria de 13/10/2026, QT48)

Como foi verificado: leitura do código contra cada pedido seu + 224 testes do núcleo Rust (passam) + build Qt compila.
**Nada disso foi executado no Windows por mim** — "✔ código" significa: existe, compila, foi visto em captura sem janela
ou coberto por teste. Só você confirma o comportamento real. `✔` código pronto · `◐` parcial · `✘` falta · `?` pronto mas nunca confirmado por você.

Decisão: **sem HEIC** (removido do seletor e do código). Todas as imagens guardadas pelo app são **WebP**.

## Imagens / WebP
- ✔ Capas, fundos e wallpapers baixados são guardados como WebP em `<dados>/pictures` e lidos de lá (offline)
- ✔ Leitura de WebP (decoder próprio do núcleo quando o plugin do Qt falta)
- ✔ Capa/fundo escolhidos por você e capas de mangá/novel → convertidos para WebP (qualidade 88–90)
- ✔ Páginas gigantes (webtoon) limitadas a 8192 px antes do WebP
- ✔ HEIC removido (seletor, mensagens e comentários)
- ◐ Páginas de mangá lidas pelo leitor: cache em WebP só nos itens que passam pelo provedor de imagens; downloads de capítulo no disco continuam no formato original da fonte

## Modo Vídeo
- ✔ Descobrir, plataformas, categorias/gêneros, busca, biblioteca (cartões ricos), favoritos, continuar assistindo (blur + zoom no play + luz do accent)
- ✔ Detalhe: streams, selos, marcar assistido (episódio/até aqui/temporada), chips de tempo restante, capa/fundo próprios, miniaturas dos episódios, baixar episódios
- ✔ Info estilo MAL; TMDB só como reserva (filmes/séries) com comentários, recomendados/similares; cache para itens da biblioteca/favoritos
- ✔ Calendário com clique no dia · sino de novos episódios/capítulos + selo NEW · painel de tarefas
- ✔ Player: qualidade/áudio/legendas/velocidade/Anime4K/escala, miniaturas de seek, lista de episódios lateral, estatísticas, atalhos configuráveis
- ✔ Pular abertura/encerramento + botão pílula "Auto skip" ao lado do Skip
- ✔ Legendas em colunas, posição/margem, tradução forçada da legenda
- ✔ Karaokê (Karaoke Mugen): variantes, atraso, cor/fonte/tamanho/contorno/sombra/margem próprios (não copia mais a legenda)
- ✔ Real-Debrid, Anime Skip, MAL login, AniSync, addons (instalar por link/remover/ligar)
- ✔ Tema do player por tema (GNOME/Apple/Windows/Android) — QT47
- ? Torrents abrindo e botão voltar (você relatou problema antes do QT7; depois disse que está funcionando)
- ✔ Estúdio de karaokê (QT49): todas as configurações de estilo ASS (fonte, tamanho, negrito/itálico/sublinhado, escala, espaçamento, 4 cores, borda/sombra/caixa, posição 1-9, margens, atraso global e por música) com prévia ao vivo e estilos prontos

## Modo Imagens (mangá) e Texto (novels)
- ✔ Explorar por idioma/fonte, biblioteca com categorias, histórico, série, fontes Suwayomi, repositórios por link, botão "Image sources"/"Text sources"
- ✔ Leitor de mangá: página/rolagem, zoom até valores pequenos, upscaling mantendo proporção + barra de progresso, tamanho do texto traduzido, seleção de texto/balões
- ✔ Tradução de página: OCR (OCR.space/Google Vision) + texto encaixado no balão + botão atualizar + progresso
- ✔ Novels: plugins JS (LNReader/IReader), leitor, edição/importação de livros, tradução em paralelo (3 sessões, ajustável em translate::READER_THREADS) com cache em disco
- ✔ Retry automático quando o Suwayomi está fora do ar (QT42)
- ✔ Fontes das Preferências carregam aos poucos (sem travar)
- ? Travamento ao abrir mangá: causa não confirmada (limite de textura 8192 aplicado; diagnósticos em debug/); aguardo seus logs
- ? Tradutores de texto/legenda: vários provedores falhavam (502/TLS/403) — sem resposta sua sobre trocar servidores
- ◐ Tradução de mangá no nível Koharu/MangaAMTL (detecção de balão local, apagar texto original/inpainting): não há análise registrada nem código; hoje depende de OCR online
- ✘ Reader zoom/gestos de toque avançados; páginas WebP offline por capítulo

## Modo Estudo (Anki)
- ✔ Baralhos, revisão (Again/Hard/Good/Easy), desfazer, suspender, enterrar, bandeira, marcar, resetar, data de vencimento
- ✔ Criar/editar/apagar notas, navegador com busca simples, estatísticas, importar .apkg (anki2/anki21)
- ✔ Editor do player: recorta trecho + legenda + kanji/hiragana/katakana + descrição da web → baralho `Reelhouse::Clips`
- ✔ Flashcard automático no canto inferior direito, 5 s, junto da legenda
- ✘ Pendentes (docs/anki-checklist.md): opções do baralho, info do cartão, tags, tela de visão geral, sintaxe de busca, áudio/TTS, exportar, .apkg anki21b, baralhos filtrados, FSRS, verbos conjugados no flashcard, tocar o clipe dentro do cartão
- ? Clipe com libx264 no Windows, consultas Jisho/Wikipedia: testados aqui, não no seu PC

## Aparência e janela
- ✔ Temas GNOME, Apple (Lucide), Windows, Android; look uniforme/portal Adwaita + accent do sistema
- ✔ Aurora, filtro noturno, zoom da interface, controle (gamepad), botões laterais do mouse, idioma da interface (PT/ES)
- ✔ Pasta `debug/` com todos os logs (+ crash.txt, crash-signal.txt, crash-rust.txt, log de build)
- ✔ Backup único (configurações, logins, bibliotecas, Estudo, imagens escolhidas) manual e automático

## Próximo (sugestão)
1. Você me manda `debug/` do crash do mangá → corrijo de vez
2. Decidir os provedores de tradução (trocar servidores / limitar paralelismo)
3. Itens ✘ do Estudo (opções do baralho, tags, busca, exportar)
4. Tradução de mangá local estilo Koharu (maior esforço)
