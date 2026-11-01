//! The macaw's ("Arara") funny lines, Duolingo-style: playful guilt trips, drama, self-aware jokes and puns about
//! anime, manga, novels and studying. Never mean. See docs/mascot-contract.md.
//!
//! Each line is (English, Portuguese (Brazil), Spanish); "{n}" is a number filled in by the caller (days of streak,
//! XP left, cards due, steps left, days away). The macaw is "a arara" (feminine) in Portuguese.
//!
//! Two kinds of moments:
//! - the study reminders of notify_study.rs (banners): "streak", "goal", "review", "quest", "idle", "greet_morning",
//!   "greet_afternoon", "greet_evening", "greet_night", "greet_streak" — these lines are extra bodies for the pools
//!   there (which keep their own titles);
//! - the window's moments, asked by QML through `Core.state("mascot:<lang>:<moment>")` → {text}: see `MOMENTS`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// (en, pt-BR, es)
pub type Quip = (&'static str, &'static str, &'static str);

/// The moments the window asks for (the banner moments are listed in the module docs).
pub const MOMENTS: &[&str] = &[
    "review_right",   // StudyReview: a right answer (Good / Easy), now and then
    "review_wrong",   // StudyReview: Again
    "lesson_finish",  // LearnLesson: the finish screen
    "lesson_perfect", // LearnLesson: finished without a mistake
    "learn_cheer",    // LearnPage: beside the current lesson
    "study_hello",    // StudyPage: no decks yet
    "buddy_done",     // StudyBuddy: every card of today reviewed
    "buddy_empty",    // StudyBuddy: nothing due, nothing reviewed
    "chapter_done",   // manga / novel reader: a chapter finished
    "tap",            // StudyBuddy: the macaw is clicked
    "quiz_right",     // ReaderQuizCard: right answer
    "quiz_wrong",     // ReaderQuizCard: wrong answer
];

const STREAK: &[Quip] = &[
    ("Your {n}-day streak just asked me if you still love it. I didn't know what to say.", "Sua ofensiva de {n} dias me perguntou se você ainda gosta dela. Eu não soube o que dizer.", "Tu racha de {n} días me preguntó si todavía la quieres. No supe qué decirle."),
    ("I'm not saying I'll cry if you lose {n} days. I'm saying I've already bought tissues.", "Não estou dizendo que vou chorar se você perder {n} dias. Só estou dizendo que já comprei lenços.", "No digo que vaya a llorar si pierdes {n} días. Solo digo que ya compré pañuelos."),
    ("{n} days. It would be a shame if something… happened to them. One lesson and nothing happens.", "{n} dias. Seria uma pena se algo… acontecesse com eles. Uma lição e nada acontece.", "{n} días. Sería una pena que algo… les pasara. Una lección y no pasa nada."),
    ("The streak fire is down to a tiny candle. Blow gently. With a lesson.", "O fogo da ofensiva virou uma velinha. Assopra com cuidado. Com uma lição.", "El fuego de la racha quedó en una velita. Sopla con cuidado. Con una lección."),
    ("Plot twist: the hero saves the {n}-day streak at the last minute. The hero is you.", "Reviravolta: o protagonista salva a ofensiva de {n} dias no último minuto. O protagonista é você.", "Giro de guion: el protagonista salva la racha de {n} días en el último minuto. El protagonista eres tú."),
    ("This is the part of the anime where the music gets sad. Change the soundtrack: one lesson.", "Essa é a parte do anime em que a música fica triste. Muda a trilha sonora: uma lição.", "Esta es la parte del anime donde la música se pone triste. Cambia la banda sonora: una lección."),
    ("I drank three energy drinks to guard your {n} days. My heart is doing 200 bpm. Help.", "Tomei três energéticos para vigiar seus {n} dias. Meu coração está a 200 bpm. Socorro.", "Me tomé tres energéticas para vigilar tus {n} días. Mi corazón va a 200 lpm. Ayuda."),
    ("Your streak sent you a message: \"u up?\"", "Sua ofensiva mandou mensagem: \"tá acordado?\"", "Tu racha te mandó un mensaje: \"¿despierto?\""),
    ("Midnight is coming, and your streak is not a Cinderella I want to see run away.", "A meia-noite está chegando, e sua ofensiva não é uma Cinderela que eu quero ver fugir.", "Se acerca la medianoche, y tu racha no es una Cenicienta que quiera ver huir."),
    ("{n} days of effort vs. five minutes of lesson. The math is on your side.", "{n} dias de esforço contra cinco minutos de lição. A matemática está do seu lado.", "{n} días de esfuerzo contra cinco minutos de lección. Las matemáticas están de tu lado."),
    ("I'm not being dramatic. I'm just lying on the floor thinking about your streak.", "Eu não sou dramática. Só estou deitada no chão pensando na sua ofensiva.", "No hago drama. Solo estoy tirado en el suelo pensando en tu racha."),
    ("Previously on your streak: {n} amazing days. Next episode: up to you.", "Anteriormente na sua ofensiva: {n} dias incríveis. Próximo episódio: depende de você.", "Anteriormente en tu racha: {n} días increíbles. Próximo episodio: depende de ti."),
    ("Your streak is like a tamagotchi. Feed it a lesson before it gets sad.", "Sua ofensiva é tipo um tamagotchi. Dá uma lição pra ela antes que fique triste.", "Tu racha es como un tamagotchi. Dale una lección antes de que se ponga triste."),
    ("The flame is flickering. I'm shielding it with my wings, but my wings are tired.", "A chama está tremendo. Estou protegendo com as asas, mas minhas asas estão cansadas.", "La llama está temblando. La cubro con mis alas, pero mis alas están cansadas."),
    ("One lesson. Just one. I'll even stop looking at you like this.", "Uma lição. Só uma. Eu até paro de te olhar desse jeito.", "Una lección. Solo una. Hasta dejo de mirarte así."),
    ("Breaking news: local macaw refuses to sleep until your {n}-day streak is safe.", "Urgente: arara local se recusa a dormir até sua ofensiva de {n} dias estar segura.", "Última hora: guacamayo local se niega a dormir hasta que tu racha de {n} días esté a salvo."),
    ("Every great saga has filler episodes. Today can't be the one where the streak dies.", "Toda grande saga tem episódio fíler. Hoje não pode ser o episódio em que a ofensiva morre.", "Toda gran saga tiene episodios de relleno. Hoy no puede ser el episodio en que muere la racha."),
    ("Remember day one? I do. I cried a little. Let's not throw away {n} days.", "Lembra do primeiro dia? Eu lembro. Chorei um pouquinho. Não vamos jogar fora {n} dias.", "¿Recuerdas el primer día? Yo sí. Lloré un poquito. No tiremos {n} días a la basura."),
    ("I could guilt-trip you. Or I could say \"please\" with very big eyes. Please.", "Eu podia fazer chantagem emocional. Ou podia pedir \"por favor\" com olhos bem grandes. Por favor.", "Podría hacerte chantaje emocional. O pedir \"por favor\" con ojos muy grandes. Por favor."),
    ("Your streak is at 1% battery. You are the charger.", "Sua ofensiva está com 1% de bateria. Você é o carregador.", "Tu racha está al 1% de batería. Tú eres el cargador."),
    ("Even isekai heroes study the new world's language. Just saying.", "Até protagonista de isekai estuda a língua do novo mundo. Só comentando.", "Hasta el protagonista de un isekai estudia el idioma del nuevo mundo. Solo digo."),
    ("Save the streak now and I'll stop sending these. Tonight. Probably.", "Salva a ofensiva agora e eu paro de mandar isso. Hoje. Provavelmente.", "Salva la racha ahora y dejo de mandarte esto. Por hoy. Probablemente."),
];

const GOAL: &[Quip] = &[
    ("{n} XP. That's shorter than an opening theme. Skip the intro, do a lesson.", "{n} XP. É menos que uma abertura de anime. Pula a abertura e faz uma lição.", "{n} XP. Es menos que un opening. Sáltate la intro y haz una lección."),
    ("Your daily goal is {n} XP away and it keeps staring at me. I can't take the staring.", "Sua meta diária está a {n} XP e fica me encarando. Não aguento mais essa encarada.", "Tu meta diaria está a {n} XP y no deja de mirarme. No aguanto la mirada."),
    ("Fun fact: {n} XP fit perfectly between two episodes.", "Curiosidade: {n} XP cabem certinho entre dois episódios.", "Dato curioso: {n} XP caben justo entre dos episodios."),
    ("I promised the goal you'd come back. Don't make me look bad.", "Prometi pra meta que você voltaria. Não me faça passar vergonha.", "Le prometí a la meta que volverías. No me hagas quedar mal."),
    ("The goal bar is \"almost full\", like my energy drink fridge. Let's fill one of them.", "A barra da meta está \"quase cheia\", igual minha geladeira de energético. Vamos encher uma das duas.", "La barra de la meta está \"casi llena\", como mi nevera de energéticas. Llenemos una de las dos."),
    ("{n} XP to go. I'd do it for you, but I have wings, not thumbs.", "Faltam {n} XP. Eu faria por você, mas tenho asas, não polegares.", "Faltan {n} XP. Lo haría por ti, pero tengo alas, no pulgares."),
    ("Today's goal is like a manga chapter on a cliffhanger. Finish it.", "A meta de hoje está igual capítulo de mangá no cliffhanger. Termina.", "La meta de hoy es como un capítulo de manga en cliffhanger. Termínalo."),
    ("You chose this goal. I just remember it. Loudly. {n} XP left.", "Você escolheu essa meta. Eu só lembro dela. Bem alto. Faltam {n} XP.", "Tú elegiste esta meta. Yo solo la recuerdo. En voz alta. Faltan {n} XP."),
    ("A tiny lesson now, a tiny victory dance later. I've been rehearsing.", "Uma lição pequenininha agora, uma dancinha da vitória depois. Já estou ensaiando.", "Una lección chiquita ahora, un bailecito de victoria después. Ya lo estoy ensayando."),
    ("{n} XP. You've binged twelve episodes in one sitting. You can do this.", "{n} XP. Você já maratonou doze episódios de uma vez. Isso aqui você tira de letra.", "{n} XP. Ya maratoneaste doce episodios de una sentada. Esto es pan comido."),
    ("Your XP bar asked me if it's ugly. It's not ugly, it's just incomplete.", "Sua barra de XP me perguntou se ela é feia. Não é feia, só está incompleta.", "Tu barra de XP me preguntó si es fea. No es fea, solo está incompleta."),
    ("Level up your brain. Side effects include knowing more words. Terrible, I know.", "Suba o nível do cérebro. Efeitos colaterais: saber mais palavras. Terrível, eu sei.", "Sube de nivel el cerebro. Efectos secundarios: saber más palabras. Terrible, lo sé."),
    ("{n} XP and I'll stop hovering over your screen. Pinky promise. I don't have pinkies.", "{n} XP e eu paro de rondar sua tela. Promessa de dedinho. Eu não tenho dedinho.", "{n} XP y dejo de rondar tu pantalla. Promesa de meñique. No tengo meñique."),
    ("The goal is so close I can smell it. It smells like victory and a little like coffee.", "A meta está tão perto que dá pra sentir o cheiro. Cheira a vitória e um pouco a café.", "La meta está tan cerca que puedo olerla. Huele a victoria y un poco a café."),
];

const REVIEW: &[Quip] = &[
    ("{n} cards are waiting like fans outside a concert. Go say hi.", "{n} cartões esperando igual fã na porta do show. Vai lá dar um oi.", "{n} tarjetas esperando como fans a la salida del concierto. Ve a saludar."),
    ("Your cards formed a union. Their demand: {n} reviews today.", "Seus cartões fundaram um sindicato. A exigência: {n} revisões hoje.", "Tus tarjetas formaron un sindicato. Su exigencia: {n} repasos hoy."),
    ("The forgetting curve is coming for {n} cards. I can see it. It has teeth.", "A curva do esquecimento está vindo atrás de {n} cartões. Eu estou vendo. Ela tem dentes.", "La curva del olvido va por {n} tarjetas. La estoy viendo. Tiene dientes."),
    ("{n} words are packing their bags to leave your brain. Stop them at the door.", "{n} palavras estão fazendo as malas pra sair do seu cérebro. Segura elas na porta.", "{n} palabras están haciendo las maletas para irse de tu cerebro. Detenlas en la puerta."),
    ("Anki called. Well, it didn't call, it just piled up {n} cards. Same thing.", "O Anki ligou. Quer dizer, não ligou, só acumulou {n} cartões. Dá no mesmo.", "Anki llamó. Bueno, no llamó, solo acumuló {n} tarjetas. Es lo mismo."),
    ("{n} cards. Think of it as gacha pulls, but every pull is a guaranteed word.", "{n} cartões. Pensa como gacha, só que todo giro é palavra garantida.", "{n} tarjetas. Piénsalo como un gacha, pero cada tirada es una palabra garantizada."),
    ("Your cards are like a slow-burn romance. They need attention every day.", "Seus cartões são tipo romance slow burn. Precisam de atenção todo dia.", "Tus tarjetas son como un romance a fuego lento. Necesitan atención todos los días."),
    ("I counted {n} cards. Then I counted again. Still {n}. They don't review themselves.", "Contei {n} cartões. Contei de novo. Continuam {n}. Eles não se revisam sozinhos.", "Conté {n} tarjetas. Volví a contar. Siguen siendo {n}. No se repasan solas."),
    ("Skip today and tomorrow it's {n} plus a lot more. That's a horror anime, not a study plan.", "Se pular hoje, amanhã são {n} e mais um monte. Isso é anime de terror, não plano de estudo.", "Si lo saltas hoy, mañana serán {n} y muchas más. Eso es un anime de terror, no un plan de estudio."),
    ("Quick review now = fewer sad cards later. I made a chart. It's in my head.", "Revisão rápida agora = menos cartões tristes depois. Fiz um gráfico. Está na minha cabeça.", "Repaso rápido ahora = menos tarjetas tristes después. Hice un gráfico. Está en mi cabeza."),
    ("Past you wrote these {n} cards with so much hope. Don't ghost them.", "O você do passado escreveu esses {n} cartões com tanta esperança. Não dá ghost neles.", "Tu yo del pasado escribió estas {n} tarjetas con tanta ilusión. No les hagas ghosting."),
    ("{n} cards and a macaw who believes in you. Good odds.", "{n} cartões e uma arara que acredita em você. As chances são boas.", "{n} tarjetas y un guacamayo que cree en ti. Buenas probabilidades."),
    ("Flashcards: the only cards that make you stronger without a booster pack.", "Flashcards: as únicas cartas que te deixam mais forte sem precisar de booster.", "Flashcards: las únicas cartas que te hacen más fuerte sin sobres de mejora."),
    ("The cards miss you so much one of them wrote a poem. It's three words long. Go read it.", "Os cartões estão com tanta saudade que um deles escreveu um poema. Tem três palavras. Vai ler.", "Las tarjetas te extrañan tanto que una escribió un poema. Tiene tres palabras. Ve a leerlo."),
    ("That word you swore you'd never forget? It's in the pile of {n}. Awkward.", "Aquela palavra que você jurava que nunca ia esquecer? Está na pilha de {n}. Constrangedor.", "¿Esa palabra que jurabas que nunca olvidarías? Está en la pila de {n}. Incómodo."),
    ("Ten minutes of reviews, then back to your show. I'll hold the remote.", "Dez minutinhos de revisão e depois volta pra sua série. Eu seguro o controle.", "Diez minutitos de repaso y luego vuelves a tu serie. Yo sostengo el control."),
];

const QUEST: &[Quip] = &[
    ("Only {n} more. The quest is basically begging.", "Só mais {n}. A missão está praticamente implorando.", "Solo {n} más. La misión prácticamente te está suplicando."),
    ("Stopping now is like dropping a manga one chapter before the finale.", "Parar agora é tipo largar o mangá um capítulo antes do final.", "Parar ahora es como dejar un manga un capítulo antes del final."),
    ("{n} more and you get the shiny thing. I love shiny things. I'm a bird.", "Mais {n} e você ganha a coisa brilhante. Eu amo coisas brilhantes. Sou uma ave.", "{n} más y te llevas la cosa brillante. Me encantan las cosas brillantes. Soy un ave."),
    ("The quest bar is at \"so close it hurts\". Make it stop hurting.", "A barra da missão está em \"tão perto que dói\". Faz parar de doer.", "La barra de la misión está en \"tan cerca que duele\". Haz que deje de doler."),
    ("Side quest available: finish this one in {n} steps. Reward: my respect. And gems.", "Missão secundária disponível: termine esta em {n} passos. Recompensa: meu respeito. E gemas.", "Misión secundaria disponible: termina esta en {n} pasos. Recompensa: mi respeto. Y gemas."),
    ("You've done the hard part. The easy part is {n} tiny steps. Go, go, go!", "Você já fez a parte difícil. A parte fácil são {n} passinhos. Vai, vai, vai!", "Ya hiciste lo difícil. Lo fácil son {n} pasitos. ¡Vamos, vamos, vamos!"),
    ("Final boss HP: {n}. Your HP: full. You know what to do.", "Vida do chefão final: {n}. Sua vida: cheia. Você sabe o que fazer.", "Vida del jefe final: {n}. Tu vida: llena. Ya sabes qué hacer."),
    ("I already ordered confetti for this quest. Non-refundable. {n} more, please.", "Já encomendei confete pra essa missão. Sem reembolso. Mais {n}, por favor.", "Ya encargué confeti para esta misión. Sin reembolso. {n} más, por favor."),
    ("A quest stuck at 90% is the saddest thing I know. Right after anime with no season two.", "Missão parada em 90% é a coisa mais triste que eu conheço. Logo depois de anime sem segunda temporada.", "Una misión atascada al 90% es lo más triste que conozco. Justo después del anime sin segunda temporada."),
    ("{n} to go. You could finish it before the next episode loads.", "Faltam {n}. Dá pra terminar antes do próximo episódio carregar.", "Faltan {n}. Puedes terminarla antes de que cargue el próximo episodio."),
];

const IDLE: &[Quip] = &[
    ("{n} days. I've been talking to your flashcards. They're nice, but they're not you.", "{n} dias. Andei conversando com seus cartões. Eles são legais, mas não são você.", "{n} días. He estado hablando con tus tarjetas. Son simpáticas, pero no eres tú."),
    ("I'm not mad. I'm just a macaw sitting alone with {n} days of energy drinks.", "Não estou brava. Só sou uma arara sozinha com {n} dias de energético acumulado.", "No me enojé. Solo soy un guacamayo a solas con {n} días de energéticas acumuladas."),
    ("It's been {n} days. I started a podcast about it. One listener: me.", "Faz {n} dias. Comecei um podcast sobre isso. Um ouvinte: eu.", "Han pasado {n} días. Empecé un podcast sobre eso. Un oyente: yo."),
    ("Your words are getting dusty. I tried to dust them. Wrong tool: wings.", "Suas palavras estão pegando poeira. Tentei tirar o pó. Ferramenta errada: asas.", "Tus palabras están juntando polvo. Intenté limpiarlas. Herramienta equivocada: alas."),
    ("Did I do something wrong? Was it the notifications? It was the notifications, wasn't it.", "Eu fiz alguma coisa errada? Foram as notificações? Foram as notificações, né.", "¿Hice algo mal? ¿Fueron las notificaciones? Fueron las notificaciones, ¿verdad?"),
    ("{n} days without you feels like a season break. I'm watching the recap on loop.", "{n} dias sem você parece hiato de temporada. Estou vendo o resumo sem parar.", "{n} días sin ti es como un parón de temporada. Estoy viendo el resumen una y otra vez."),
    ("I made a \"Missing\" poster with your face. Then I remembered I can just send a banner.", "Fiz um cartaz de \"Desaparecido\" com a sua cara. Aí lembrei que dava só pra mandar um aviso.", "Hice un cartel de \"Se busca\" con tu cara. Luego recordé que podía mandar un aviso."),
    ("Come back for two minutes. I'll pretend I wasn't counting the days. ({n}. It was {n}.)", "Volta por dois minutinhos. Eu finjo que não estava contando os dias. ({n}. Foram {n}.)", "Vuelve dos minutitos. Fingiré que no contaba los días. ({n}. Fueron {n}.)"),
    ("The flashback episode of us studying together is my favourite. Let's film a new one.", "O episódio de flashback da gente estudando junto é meu favorito. Vamos gravar um novo.", "El episodio de flashback de nosotros estudiando juntos es mi favorito. Grabemos uno nuevo."),
    ("I kept your seat warm for {n} days. My feathers are tired. Sit down.", "Guardei seu lugar quentinho por {n} dias. Minhas penas estão cansadas. Senta aí.", "Te guardé el asiento calentito {n} días. Mis plumas están cansadas. Siéntate."),
    ("Comeback arcs are the best arcs. Start yours with one lesson.", "Arco de retorno é o melhor arco. Começa o seu com uma lição.", "Los arcos de regreso son los mejores. Empieza el tuyo con una lección."),
    ("No judgment. Okay, a little judgment. Mostly love. Come back?", "Sem julgamentos. Tá, um pouquinho de julgamento. Mas mais amor. Volta?", "Sin juicios. Bueno, un poquito de juicio. Sobre todo cariño. ¿Vuelves?"),
    ("Your kana keep asking about you. Especially ぬ. ぬ is always dramatic.", "Seus kana vivem perguntando por você. Principalmente o ぬ. O ぬ é sempre dramático.", "Tus kana no paran de preguntar por ti. Sobre todo ぬ. ぬ siempre es dramático."),
    ("I got so bored I sorted my energy drinks alphabetically. Please give me something to do.", "Fiquei tão entediada que organizei os energéticos em ordem alfabética. Me dá algo pra fazer, por favor.", "Me aburrí tanto que ordené las energéticas por orden alfabético. Por favor, dame algo que hacer."),
    ("{n} days is a long time in macaw years. I've aged. Visibly.", "{n} dias é muito tempo em anos de arara. Envelheci. Dá pra ver.", "{n} días es mucho tiempo en años de guacamayo. Envejecí. Se nota."),
    ("One tiny lesson and we pretend this never happened. Deal?", "Uma liçãozinha e a gente finge que isso nunca aconteceu. Combinado?", "Una leccioncita y fingimos que esto nunca pasó. ¿Trato?"),
];

const GREET_MORNING: &[Quip] = &[
    ("Rise and shine! I'm on energy drink number one. You're on lesson number one, right?", "Bom dia, flor do dia! Eu estou no energético número um. Você está na lição número um, né?", "¡Arriba! Voy por la energética número uno. Tú vas por la lección número uno, ¿verdad?"),
    ("Fun morning fact: brains learn better after sleep. Yours is freshly charged.", "Curiosidade matinal: o cérebro aprende melhor depois de dormir. O seu está carregadinho.", "Dato mañanero: el cerebro aprende mejor después de dormir. El tuyo está recién cargado."),
    ("Good morning! I fluffed my feathers for you. That's macaw for \"let's study\".", "Bom dia! Afofei as penas pra você. Em araranês isso quer dizer \"bora estudar\".", "¡Buenos días! Esponjé las plumas para ti. En idioma guacamayo eso significa \"a estudiar\"."),
    ("Breakfast idea: coffee, toast and five new words.", "Ideia de café da manhã: café, pão na chapa e cinco palavras novas.", "Idea de desayuno: café, tostadas y cinco palabras nuevas."),
    ("I've been up since 5. Not by choice. The energy drink chose for me.", "Estou acordada desde as 5. Não foi escolha minha. O energético escolheu por mim.", "Llevo en pie desde las 5. No fue mi decisión. La energética decidió por mí."),
    ("Morning! Today's forecast: 100% chance of learning something cool.", "Bom dia! Previsão de hoje: 100% de chance de aprender algo legal.", "¡Buen día! Pronóstico de hoy: 100% de probabilidad de aprender algo genial."),
    ("Opening theme of the day: you, me and a quick lesson. Cue the music!", "Abertura do dia: você, eu e uma lição rápida. Solta a música!", "Opening del día: tú, yo y una lección rápida. ¡Que suene la música!"),
];

const GREET_AFTERNOON: &[Quip] = &[
    ("Post-lunch sleepiness detected. Cure: one lesson. Side effect: smarter.", "Sonolência pós-almoço detectada. Cura: uma lição. Efeito colateral: ficar mais esperto.", "Sueño de después de comer detectado. Cura: una lección. Efecto secundario: más listo."),
    ("Afternoon! I saved you a seat between the anime and the flashcards.", "Boa tarde! Guardei seu lugar entre o anime e os flashcards.", "¡Buenas tardes! Te guardé un sitio entre el anime y las flashcards."),
    ("It's the perfect hour for a lesson. I checked. I'm a clock now.", "É a hora perfeita pra uma lição. Eu conferi. Agora sou um relógio.", "Es la hora perfecta para una lección. Lo comprobé. Ahora soy un reloj."),
    ("Coffee break? Word break! Same thing, fewer calories.", "Pausa pro café? Pausa pras palavras! Mesma coisa, menos calorias.", "¿Pausa para el café? ¡Pausa para palabras! Lo mismo, menos calorías."),
    ("Hi! I was just pretending to be busy. Let's actually be busy together.", "Oi! Eu estava só fingindo que estava ocupada. Vamos ficar ocupados de verdade juntos.", "¡Hola! Solo hacía como que trabajaba. Trabajemos de verdad juntos."),
    ("Afternoon plot: the hero opens the app, sees the macaw, does a lesson. Great writing.", "Roteiro da tarde: o herói abre o app, vê a arara e faz uma lição. Que roteiro.", "Guion de la tarde: el héroe abre la app, ve al guacamayo y hace una lección. Qué guion."),
    ("You came back! I knew you would. I also knew it would be around now. I'm psychic.", "Você voltou! Eu sabia. Também sabia que seria mais ou menos agora. Sou vidente.", "¡Volviste! Lo sabía. También sabía que sería más o menos ahora. Soy vidente."),
];

const GREET_EVENING: &[Quip] = &[
    ("Evening! The best episodes come after a quick lesson. That's science. Probably.", "Boa noite! Os melhores episódios vêm depois de uma lição rápida. É ciência. Provavelmente.", "¡Buenas noches! Los mejores episodios vienen después de una lección rápida. Es ciencia. Probablemente."),
    ("Dinner, a show and five minutes of study. The perfect combo.", "Jantar, uma série e cinco minutos de estudo. O combo perfeito.", "Cena, una serie y cinco minutos de estudio. El combo perfecto."),
    ("Long day? I'll keep it short. Like my attention span.", "Dia longo? Vou ser breve. Igual minha capacidade de atenção.", "¿Día largo? Seré breve. Como mi capacidad de atención."),
    ("The sun is down, the macaw is up. As usual.", "O sol se pôs, e a arara continua acordada. Como sempre.", "Se puso el sol y el guacamayo sigue despierto. Como siempre."),
    ("Evening plan: blanket, snacks, subtitles in the language you're learning. Fancy!", "Plano pra noite: coberta, petiscos e legenda no idioma que você está aprendendo. Chique!", "Plan de la noche: manta, snacks y subtítulos en el idioma que aprendes. ¡Qué nivel!"),
    ("Hey! Before the next episode: one tiny lesson? For me? I'm adorable.", "Ei! Antes do próximo episódio: uma liçãozinha? Por mim? Eu sou fofa.", "¡Oye! Antes del próximo episodio: ¿una leccioncita? ¿Por mí? Soy adorable."),
    ("Welcome home. I tidied up the flashcards. By tidied, I mean counted them nervously.", "Bem-vindo de volta. Arrumei os flashcards. Por arrumar, quero dizer que contei eles nervosa.", "Bienvenido. Ordené las flashcards. Por ordenar, quiero decir que las conté con nervios."),
];

const GREET_NIGHT: &[Quip] = &[
    ("It's late. I'm judging you. Gently. From the dark.", "Está tarde. Estou te julgando. Com carinho. Do escuro.", "Es tarde. Te estoy juzgando. Con cariño. Desde la oscuridad."),
    ("\"Just one more episode,\" said everyone, every night, forever.", "\"Só mais um episódio\", disse todo mundo, toda noite, pra sempre.", "\"Solo un episodio más\", dijo todo el mundo, cada noche, para siempre."),
    ("Night owl? Pfft. Night macaw is way cooler. Welcome to the club.", "Coruja da noite? Pff. Arara da noite é muito mais legal. Bem-vindo ao clube.", "¿Búho nocturno? Bah. Guacamayo nocturno suena mucho mejor. Bienvenido al club."),
    ("Fun fact: sleep saves what you learned. So learn one thing, then sleep. Win-win.", "Curiosidade: o sono salva o que você aprendeu. Então aprende uma coisa e vai dormir. Todo mundo ganha.", "Dato curioso: dormir guarda lo que aprendiste. Así que aprende algo y luego duerme. Todos ganan."),
    ("Shh, everyone's asleep. Perfect time to whisper new words.", "Psiu, todo mundo dormindo. Hora perfeita pra sussurrar palavras novas.", "Shh, todos duermen. Momento perfecto para susurrar palabras nuevas."),
    ("I'm on my fourth energy drink. What's your excuse?", "Estou no quarto energético. Qual é a sua desculpa?", "Voy por la cuarta energética. ¿Cuál es tu excusa?"),
    ("Midnight snack idea: one flashcard. Zero calories.", "Ideia de lanchinho da madrugada: um flashcard. Zero calorias.", "Idea de picoteo de medianoche: una flashcard. Cero calorías."),
];

const GREET_STREAK: &[Quip] = &[
    ("Day {n}! By now your streak has its own fan club. I'm the president.", "Dia {n}! A essa altura sua ofensiva tem fã-clube. Eu sou a presidente.", "¡Día {n}! A estas alturas tu racha tiene club de fans. Yo lo presido."),
    ("{n} days! Your streak is so hot I need sunscreen.", "{n} dias! Sua ofensiva está tão quente que preciso de protetor solar.", "¡{n} días! Tu racha está tan caliente que necesito protector solar."),
    ("Welcome back, {n}-day legend. Don't let it go to your head. Okay, a little.", "Bem-vindo de volta, lenda de {n} dias. Não deixa subir à cabeça. Tá, só um pouquinho.", "Bienvenido de nuevo, leyenda de {n} días. Que no se te suba a la cabeza. Bueno, un poquito."),
    ("Day {n}. I told all the other macaws about you. They're jealous.", "Dia {n}. Contei de você pra todas as outras araras. Estão com inveja.", "Día {n}. Les hablé de ti a todos los otros guacamayos. Tienen envidia."),
    ("{n} days in a row! You're more consistent than my sleep schedule.", "{n} dias seguidos! Você é mais constante que meu horário de sono.", "¡{n} días seguidos! Eres más constante que mi horario de sueño."),
    ("Streak: {n}. Macaw pride: over 9000.", "Ofensiva: {n}. Orgulho da arara: mais de 8000.", "Racha: {n}. Orgullo del guacamayo: más de 8000."),
    ("Episode {n} of \"The Unstoppable Learner\". The ratings are through the roof.", "Episódio {n} de \"O Estudante Imparável\". A audiência está nas alturas.", "Episodio {n} de \"El Estudiante Imparable\". La audiencia está por las nubes."),
    ("{n} days! Keep going and I'll need a bigger trophy shelf.", "{n} dias! Continua assim que vou precisar de uma estante de troféus maior.", "¡{n} días! Sigue así y necesitaré una estantería de trofeos más grande."),
];

const REVIEW_RIGHT: &[Quip] = &[
    ("Look at you, remembering things!", "Olha só você, lembrando das coisas!", "¡Mírate, recordando cosas!"),
    ("Big brain energy.", "Energia de cérebro grande.", "Energía de cerebro gigante."),
    ("Your memory deserves a sequel.", "Sua memória merece uma continuação.", "Tu memoria merece una secuela."),
    ("Correct! I'm telling everyone.", "Acertou! Vou contar pra todo mundo.", "¡Correcto! Se lo voy a contar a todos."),
    ("Sugoi!", "Sugoi!", "¡Sugoi!"),
    ("That card never stood a chance.", "Esse cartão não teve a menor chance.", "Esa tarjeta no tuvo ninguna oportunidad."),
    ("Critical hit!", "Acerto crítico!", "¡Golpe crítico!"),
    ("Past you is proud. Future you too.", "O você do passado está orgulhoso. O do futuro também.", "Tu yo del pasado está orgulloso. El del futuro también."),
    ("Nailed it. With style.", "Mandou bem. E com estilo.", "Lo clavaste. Con estilo."),
    ("+1 word in the bag.", "+1 palavra no bolso.", "+1 palabra en el bolsillo."),
    ("I'd clap, but wings make it awkward.", "Eu aplaudiria, mas com asas fica esquisito.", "Aplaudiría, pero con alas queda raro."),
    ("Main character moment!", "Momento protagonista!", "¡Momento protagonista!"),
    ("Easy mode? Or are you just that good?", "Modo fácil? Ou você é bom assim mesmo?", "¿Modo fácil? ¿O eres así de bueno?"),
    ("Flawless. Like a perfect opening.", "Impecável. Igual abertura de anime perfeita.", "Impecable. Como un opening perfecto."),
];

const REVIEW_WRONG: &[Quip] = &[
    ("That card cheated. Definitely.", "Esse cartão roubou. Com certeza.", "Esa tarjeta hizo trampa. Seguro."),
    ("It's fine. I forget where I left my energy drink all the time.", "Tudo bem. Eu vivo esquecendo onde deixei meu energético.", "Tranquilo. Yo siempre olvido dónde dejé mi energética."),
    ("Even heroes lose the first fight.", "Até protagonista perde a primeira luta.", "Hasta los protagonistas pierden la primera pelea."),
    ("Plot twist! It'll come back. They always come back.", "Reviravolta! Ele vai voltar. Eles sempre voltam.", "¡Giro de guion! Volverá. Siempre vuelven."),
    ("Ouch. I felt that in my feathers.", "Ai. Senti isso nas penas.", "Auch. Lo sentí en las plumas."),
    ("Mistakes are just training arcs.", "Erro é só arco de treinamento.", "Los errores son solo arcos de entrenamiento."),
    ("Your brain was buffering. Happens.", "Seu cérebro estava carregando. Acontece.", "Tu cerebro estaba cargando. Pasa."),
    ("We don't talk about that one.", "Desse a gente não fala.", "De esa no hablamos."),
    ("Next time it's yours. I can feel it.", "Na próxima é sua. Estou sentindo.", "La próxima es tuya. Lo presiento."),
    ("*pretends not to see*", "*finge que não viu*", "*finge no haberlo visto*"),
    ("Wrong, but with confidence. I respect that.", "Errado, mas com confiança. Respeito.", "Incorrecto, pero con confianza. Lo respeto."),
    ("That word is playing hard to get.", "Essa palavra está se fazendo de difícil.", "Esa palabra se está haciendo la difícil."),
    ("Deep breath. Even I blink sometimes.", "Respira fundo. Até eu pisco às vezes.", "Respira hondo. Hasta yo parpadeo a veces."),
    ("Again? Again. It's literally the button's name.", "De novo? De novo. É literalmente o nome do botão.", "¿Otra vez? Otra vez. Es literalmente el nombre del botón."),
];

const LESSON_FINISH: &[Quip] = &[
    ("Lesson done! I'm doing my victory dance. Don't look.", "Lição feita! Estou fazendo minha dança da vitória. Não olha.", "¡Lección hecha! Estoy haciendo mi baile de la victoria. No mires."),
    ("You learned stuff! Your brain is officially heavier.", "Você aprendeu coisas! Seu cérebro está oficialmente mais pesado.", "¡Aprendiste cosas! Tu cerebro ahora pesa oficialmente más."),
    ("Another one for the highlight reel!", "Mais uma pros melhores momentos!", "¡Otra para los mejores momentos!"),
    ("That's how training arcs work!", "É assim que funciona um arco de treinamento!", "¡Así funcionan los arcos de entrenamiento!"),
    ("I'd buy you an energy drink, but I drank them all.", "Eu te pagaria um energético, mas bebi todos.", "Te invitaría a una energética, pero me las tomé todas."),
    ("Lesson cleared! Save your game.", "Lição zerada! Salva o jogo.", "¡Lección superada! Guarda la partida."),
    ("You + lessons = my favourite ship.", "Você + lições = meu casal favorito.", "Tú + lecciones = mi pareja favorita."),
    ("Achievement unlocked: being awesome.", "Conquista desbloqueada: ser incrível.", "Logro desbloqueado: ser increíble."),
    ("One step closer to watching without subtitles!", "Um passo mais perto de assistir sem legenda!", "¡Un paso más cerca de ver sin subtítulos!"),
    ("To be continued… tomorrow, right?", "Continua… amanhã, né?", "Continuará… mañana, ¿verdad?"),
    ("Future you just sent a thank-you note.", "O você do futuro acabou de mandar um bilhete de agradecimento.", "Tu yo del futuro acaba de mandar una nota de agradecimiento."),
    ("XP acquired. Feathers ruffled. Heart full.", "XP conquistado. Penas arrepiadas. Coração cheio.", "XP conseguido. Plumas erizadas. Corazón lleno."),
];

const LESSON_PERFECT: &[Quip] = &[
    ("Zero mistakes?! Who are you and what did you do with my student?", "Zero erros?! Quem é você e o que fez com meu aluno?", "¿¡Cero errores!? ¿Quién eres y qué hiciste con mi estudiante?"),
    ("Perfect run! Speedrunners fear you.", "Partida perfeita! Os speedrunners têm medo de você.", "¡Partida perfecta! Los speedrunners te temen."),
    ("Flawless victory. I need to sit down.", "Vitória impecável. Preciso me sentar.", "Victoria impecable. Necesito sentarme."),
    ("Not one mistake. I'm framing this lesson.", "Nenhum erro. Vou emoldurar essa lição.", "Ni un error. Voy a enmarcar esta lección."),
    ("100%! Is this your final form?", "100%! Essa é sua forma final?", "¡100%! ¿Es esta tu forma final?"),
    ("Perfect! I'm crying. It's the energy drink. It's not the energy drink.", "Perfeito! Estou chorando. É o energético. Não é o energético.", "¡Perfecto! Estoy llorando. Es la energética. No es la energética."),
];

const LEARN_CHEER: &[Quip] = &[
    ("This lesson won't do itself. I asked.", "Essa lição não vai se fazer sozinha. Eu perguntei.", "Esta lección no se va a hacer sola. Le pregunté."),
    ("Click the lesson. You know you want to.", "Clica na lição. Você sabe que quer.", "Haz clic en la lección. Sabes que quieres."),
    ("Five minutes now = one more joke you get in your anime.", "Cinco minutos agora = mais uma piada do anime que você entende.", "Cinco minutos ahora = un chiste más de tu anime que entiendes."),
    ("I'm staring at the next lesson. It's staring back.", "Estou encarando a próxima lição. Ela está me encarando de volta.", "Estoy mirando la próxima lección. Me devuelve la mirada."),
    ("Today's episode: you learn something. Spoiler: you win.", "Episódio de hoje: você aprende algo. Spoiler: você vence.", "Episodio de hoy: aprendes algo. Spoiler: ganas."),
    ("Grinding XP like in an RPG. But for your brain.", "Farmando XP igual RPG. Só que pro cérebro.", "Farmeando XP como en un RPG. Pero para el cerebro."),
    ("The path is long, but my wings are ready.", "O caminho é longo, mas minhas asas estão prontas.", "El camino es largo, pero mis alas están listas."),
    ("Psst. The next lesson is the fun one. They're all the fun one.", "Psiu. A próxima lição é a divertida. Todas são a divertida.", "Psst. La próxima lección es la divertida. Todas son la divertida."),
    ("Let's go! I already warmed up my wings.", "Bora! Já aqueci as asas.", "¡Vamos! Ya calenté las alas."),
    ("Your streak and I believe in you. Mostly me.", "Sua ofensiva e eu acreditamos em você. Mais eu.", "Tu racha y yo creemos en ti. Sobre todo yo."),
    ("Learning a language: the only grind with no loot boxes.", "Aprender idiomas: o único grind sem caixa de recompensa.", "Aprender idiomas: el único grind sin cajas de botín."),
    ("A lesson a day keeps the subtitles away.", "Uma lição por dia mantém as legendas longe.", "Una lección al día mantiene lejos los subtítulos."),
];

const STUDY_HELLO: &[Quip] = &[
    ("No decks yet? It's so empty in here there's an echo. Echo. Echo.", "Nenhum baralho ainda? Está tão vazio que dá eco. Eco. Eco.", "¿Ningún mazo todavía? Está tan vacío que hay eco. Eco. Eco."),
    ("Let's study together! I brought snacks.", "Vamos estudar juntos! Eu trouxe petiscos.", "¡Estudiemos juntos! Traje snacks."),
    ("Empty shelves make a sad macaw. Add a deck?", "Estante vazia deixa a arara triste. Adiciona um baralho?", "Estantes vacías ponen triste al guacamayo. ¿Agregas un mazo?"),
    ("I've been waiting here all day. No pressure. Lots of pressure.", "Estou esperando aqui o dia todo. Sem pressão. Muita pressão.", "Llevo todo el día esperando aquí. Sin presión. Mucha presión."),
    ("Every great collection starts with one deck. And one macaw.", "Toda grande coleção começa com um baralho. E uma arara.", "Toda gran colección empieza con un mazo. Y un guacamayo."),
    ("New here? I'm the macaw. I'll be reminding you of things. Forever.", "Novo por aqui? Eu sou a arara. Vou te lembrar das coisas. Pra sempre.", "¿Nuevo por aquí? Soy el guacamayo. Te voy a recordar cosas. Para siempre."),
];

const BUDDY_DONE: &[Quip] = &[
    ("All done! Go watch something. You earned it.", "Tudo feito! Vai assistir alguma coisa. Você mereceu.", "¡Todo listo! Ve a ver algo. Te lo ganaste."),
    ("Zero cards left. I'm going to nap on the empty pile.", "Zero cartões. Vou tirar um cochilo na pilha vazia.", "Cero tarjetas. Voy a echarme una siesta en la pila vacía."),
    ("Reviews cleared! Your memory sends its regards.", "Revisões zeradas! Sua memória manda lembranças.", "¡Repasos terminados! Tu memoria te manda saludos."),
    ("Done for today. See you tomorrow. I'll be right here. Waiting. Lovingly.", "Pronto por hoje. Te vejo amanhã. Vou estar bem aqui. Esperando. Com carinho.", "Listo por hoy. Nos vemos mañana. Estaré aquí. Esperando. Con cariño."),
    ("Inbox zero, but for flashcards. Legendary.", "Caixa de entrada zerada, só que de flashcards. Lendário.", "Bandeja vacía, pero de flashcards. Legendario."),
    ("The cards are defeated. The macaw is pleased.", "Os cartões foram derrotados. A arara está satisfeita.", "Las tarjetas fueron derrotadas. El guacamayo está satisfecho."),
    ("Nothing left to review. Time for an episode. Or three. I won't tell.", "Nada pra revisar. Hora de um episódio. Ou três. Eu não conto pra ninguém.", "Nada que repasar. Hora de un episodio. O tres. No se lo diré a nadie."),
    ("That was quick! Were you speedrunning?", "Foi rápido! Estava fazendo speedrun?", "¡Qué rápido! ¿Hacías speedrun?"),
];

const BUDDY_EMPTY: &[Quip] = &[
    ("Nothing due. Suspiciously calm. Add some new cards?", "Nada pendente. Calmo demais. Adiciona uns cartões novos?", "Nada pendiente. Sospechosamente tranquilo. ¿Agregas tarjetas nuevas?"),
    ("No cards today. I'm bored. Entertain me with new words?", "Nenhum cartão hoje. Estou entediada. Me diverte com palavras novas?", "Ninguna tarjeta hoy. Qué aburrimiento. ¿Me entretienes con palabras nuevas?"),
    ("Mine a few words from your next episode. I'll hold the pickaxe.", "Minera umas palavras do próximo episódio. Eu seguro a picareta.", "Minea unas palabras de tu próximo episodio. Yo sostengo el pico."),
    ("Empty queue. The calm before the flashcard storm.", "Fila vazia. A calmaria antes da tempestade de flashcards.", "Cola vacía. La calma antes de la tormenta de flashcards."),
    ("Day off for the cards. Not for curiosity, though.", "Folga pros cartões. Mas não pra curiosidade.", "Día libre para las tarjetas. Pero no para la curiosidad."),
    ("Nothing due. Perfect day to read a chapter in the original.", "Nada pendente. Dia perfeito pra ler um capítulo no original.", "Nada pendiente. Día perfecto para leer un capítulo en el original."),
];

const CHAPTER_DONE: &[Quip] = &[
    ("Chapter done! Next one? I won't tell anyone it's 2 a.m.", "Capítulo concluído! O próximo? Não conto pra ninguém que são 2 da manhã.", "¡Capítulo terminado! ¿El siguiente? No le diré a nadie que son las 2 de la mañana."),
    ("Reading in the original? Look at you, all fluent.", "Lendo no original? Olha só, todo fluente.", "¿Leyendo en el original? Mírate, todo fluido."),
    ("Cliffhanger? Don't look at me, I didn't write it.", "Cliffhanger? Não olha pra mim, não fui eu que escrevi.", "¿Cliffhanger? No me mires, yo no lo escribí."),
    ("Another chapter conquered. The author would be proud.", "Mais um capítulo conquistado. O autor ficaria orgulhoso.", "Otro capítulo conquistado. El autor estaría orgulloso."),
    ("Turn the page, collect the words. That's the way.", "Vira a página, coleta as palavras. É assim que se faz.", "Pasa la página, colecciona palabras. Así se hace."),
    ("The end! …of this chapter. Only 300 more to go.", "Fim! …deste capítulo. Faltam só uns 300.", "¡Fin! …de este capítulo. Quedan solo unos 300."),
    ("Chapter cleared! I was reading over your shoulder. Sorry.", "Capítulo zerado! Eu estava lendo por cima do seu ombro. Desculpa.", "¡Capítulo superado! Estaba leyendo por encima de tu hombro. Perdón."),
    ("That chapter was so good I'm molting.", "Esse capítulo foi tão bom que estou trocando as penas.", "Ese capítulo fue tan bueno que estoy mudando las plumas."),
    ("Reading mode: on. Bookworm mode: also on.", "Modo leitura: ativado. Modo rato de biblioteca: também.", "Modo lectura: activado. Modo ratón de biblioteca: también."),
    ("Next chapter or sleep? I vote next chapter. I'm a bad influence.", "Próximo capítulo ou dormir? Voto no próximo capítulo. Sou má influência.", "¿Siguiente capítulo o dormir? Voto por el siguiente. Soy mala influencia."),
];

const TAP: &[Quip] = &[
    ("Hey! That tickles.", "Ei! Isso faz cócegas.", "¡Oye! Eso hace cosquillas."),
    ("Poking me won't do your reviews.", "Me cutucar não vai fazer suas revisões.", "Picarme no va a hacer tus repasos."),
    ("Yes, I'm real. No, I won't do your homework.", "Sim, eu sou real. Não, não vou fazer sua lição de casa.", "Sí, soy real. No, no haré tu tarea."),
    ("Careful, the feathers are a limited edition.", "Cuidado, as penas são edição limitada.", "Cuidado, las plumas son edición limitada."),
    ("Did you just boop me? Rude. Cute, but rude.", "Você me deu um peteleco? Que grosseria. Fofa, mas grosseria.", "¿Me acabas de hacer boop? Qué grosero. Tierno, pero grosero."),
    ("I'm not a button. Okay, I'm a little bit a button.", "Eu não sou um botão. Tá, sou um pouquinho um botão.", "No soy un botón. Bueno, soy un poquito un botón."),
    ("Every click makes a flashcard due. Just kidding. Or am I?", "Cada clique deixa um cartão pendente. Brincadeira. Ou não?", "Cada clic deja una tarjeta pendiente. Es broma. ¿O no?"),
    ("Ara ara~ (it's in my name).", "Ara ara~ (está no meu nome).", "Ara ara~ (está en mi nombre)."),
    ("Shh, I'm thinking up new ways to remind you to study.", "Psiu, estou pensando em novas formas de te lembrar de estudar.", "Shh, estoy pensando nuevas formas de recordarte que estudies."),
    ("Senpai noticed me!", "Senpai me notou!", "¡Senpai me notó!"),
    ("I run on energy drinks and your progress. Mostly your progress.", "Funciono à base de energético e do seu progresso. Mais do seu progresso.", "Funciono con energéticas y tu progreso. Sobre todo tu progreso."),
    ("Click me ten more times and I'll… still be here. Forever.", "Clica mais dez vezes e eu vou… continuar aqui. Pra sempre.", "Haz clic diez veces más y seguiré… aquí. Para siempre."),
    ("I'm a blue macaw, not a green owl. Totally different vibe.", "Sou uma arara-azul, não uma coruja verde. Vibe completamente diferente.", "Soy un guacamayo azul, no un búho verde. Un rollo totalmente distinto."),
    ("Fun fact: macaws can live 60 years. That's a lot of reminders.", "Curiosidade: araras vivem até 60 anos. É muito lembrete.", "Dato curioso: los guacamayos viven hasta 60 años. Son muchos recordatorios."),
    ("Stop poking, start reviewing. I'm a poet.", "Para de cutucar e vai revisar. Sou poeta.", "Deja de picar y ponte a repasar. Soy poeta."),
    ("You found the secret macaw. Reward: one more lesson.", "Você achou a arara secreta. Prêmio: mais uma lição.", "Encontraste el guacamayo secreto. Premio: una lección más."),
];

const QUIZ_RIGHT: &[Quip] = &[
    ("You read that like a native!", "Leu isso igual nativo!", "¡Lo leíste como nativo!"),
    ("Look who doesn't need a dictionary!", "Olha quem não precisa de dicionário!", "¡Mira quién no necesita diccionario!"),
    ("Correct! The manga gods smile upon you.", "Acertou! Os deuses do mangá sorriem pra você.", "¡Correcto! Los dioses del manga te sonríen."),
    ("Word unlocked!", "Palavra desbloqueada!", "¡Palabra desbloqueada!"),
    ("That's my reader!", "Esse é o meu leitor!", "¡Ese es mi lector!"),
    ("Too easy. Next!", "Fácil demais. Próxima!", "Demasiado fácil. ¡Siguiente!"),
];

const QUIZ_WRONG: &[Quip] = &[
    ("That word is sneaky. We'll get it next time.", "Essa palavra é traiçoeira. Na próxima a gente pega.", "Esa palabra es traicionera. La próxima la atrapamos."),
    ("Close! In a parallel universe, that was right.", "Quase! Num universo paralelo isso estava certo.", "¡Casi! En un universo paralelo eso era correcto."),
    ("Even translators have bad days.", "Até tradutor tem dia ruim.", "Hasta los traductores tienen días malos."),
    ("No worries, it's going in the deck. Revenge later.", "Sem problema, vai pro baralho. Vingança depois.", "Tranquilo, va al mazo. Venganza después."),
    ("The kanji won this round.", "O kanji venceu esse round.", "El kanji ganó esta ronda."),
    ("Hmm. Let's pretend that was a typo.", "Hmm. Vamos fingir que foi erro de digitação.", "Mmm. Finjamos que fue un error de tipeo."),
];

/// The lines of a moment (empty for an unknown one).
pub fn lines(moment: &str) -> &'static [Quip] {
    match moment {
        "streak" => STREAK,
        "goal" => GOAL,
        "review" => REVIEW,
        "quest" => QUEST,
        "idle" => IDLE,
        "greet_morning" => GREET_MORNING,
        "greet_afternoon" => GREET_AFTERNOON,
        "greet_evening" => GREET_EVENING,
        "greet_night" => GREET_NIGHT,
        "greet_streak" => GREET_STREAK,
        "review_right" => REVIEW_RIGHT,
        "review_wrong" => REVIEW_WRONG,
        "lesson_finish" => LESSON_FINISH,
        "lesson_perfect" => LESSON_PERFECT,
        "learn_cheer" => LEARN_CHEER,
        "study_hello" => STUDY_HELLO,
        "buddy_done" => BUDDY_DONE,
        "buddy_empty" => BUDDY_EMPTY,
        "chapter_done" => CHAPTER_DONE,
        "tap" => TAP,
        "quiz_right" => QUIZ_RIGHT,
        "quiz_wrong" => QUIZ_WRONG,
        _ => &[],
    }
}

/// A line in `lang` ("pt", "pt-BR", "es"…; anything else is English).
pub fn in_lang(q: &Quip, lang: &str) -> &'static str {
    match lang.split(['-', '_']).next().unwrap_or("") {
        "pt" => q.1,
        "es" => q.2,
        _ => q.0,
    }
}

/// An index below `len` from `seed`, never `last` when there is a choice.
pub fn pick(len: usize, last: Option<usize>, seed: u64) -> usize {
    if len <= 1 {
        return 0;
    }
    match last {
        // one of the other len-1, shifted past `last`
        Some(l) if l < len => {
            let i = (seed % (len as u64 - 1)) as usize;
            if i >= l {
                i + 1
            } else {
                i
            }
        }
        _ => (seed % len as u64) as usize,
    }
}

fn seed() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let c = COUNTER.fetch_add(1, Ordering::Relaxed);
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7);
    // a quick mix (splitmix64) so close calls do not give close numbers
    let mut z = t ^ c.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A random line of `moment` in `lang`, never the same as the last one given for that moment ("" when unknown).
pub fn say(lang: &str, moment: &str) -> String {
    static LAST: Mutex<Option<HashMap<String, usize>>> = Mutex::new(None);
    let all = lines(moment);
    if all.is_empty() {
        return String::new();
    }
    let mut guard = LAST.lock().unwrap_or_else(|p| p.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    let i = pick(all.len(), map.get(moment).copied(), seed());
    map.insert(moment.to_string(), i);
    in_lang(&all[i], lang).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BANNER_MOMENTS: &[&str] =
        &["streak", "goal", "review", "quest", "idle", "greet_morning", "greet_afternoon", "greet_evening", "greet_night", "greet_streak"];

    #[test]
    fn plenty_of_lines_everywhere() {
        let mut total = 0;
        for m in BANNER_MOMENTS.iter().chain(MOMENTS) {
            let n = lines(m).len();
            assert!(n >= 6, "{} has only {} lines", m, n);
            total += n;
        }
        assert!(total >= 200, "only {} lines", total);
        assert!(lines("nope").is_empty());
        assert_eq!(say("pt", "nope"), "");
    }

    #[test]
    fn every_line_is_translated_and_numbers_match() {
        for m in BANNER_MOMENTS.iter().chain(MOMENTS) {
            for q in lines(m) {
                assert!(!q.0.is_empty() && !q.1.is_empty() && !q.2.is_empty(), "{}: {:?}", m, q);
                let n = q.0.matches("{n}").count();
                assert_eq!(q.1.matches("{n}").count(), n, "pt {{n}} of {:?}", q.0);
                assert_eq!(q.2.matches("{n}").count(), n, "es {{n}} of {:?}", q.0);
                // only the banner moments get a number
                if MOMENTS.contains(m) || (m.starts_with("greet_") && *m != "greet_streak") {
                    assert_eq!(n, 0, "{} has a number: {:?}", m, q.0);
                }
            }
        }
    }

    #[test]
    fn no_line_is_repeated() {
        let mut seen = std::collections::HashSet::new();
        for m in BANNER_MOMENTS.iter().chain(MOMENTS) {
            for q in lines(m) {
                assert!(seen.insert(q.0), "repeated: {}", q.0);
            }
        }
    }

    #[test]
    fn pick_never_repeats_the_last() {
        for seed in 0..200u64 {
            for last in 0..5 {
                let i = pick(5, Some(last), seed);
                assert!(i < 5 && i != last);
            }
            assert!(pick(5, None, seed) < 5);
        }
        assert_eq!(pick(1, Some(0), 3), 0);
        assert_eq!(pick(0, None, 3), 0);
        let mut prev = String::new();
        for _ in 0..50 {
            let s = say("pt-BR", "tap");
            assert!(!s.is_empty() && s != prev);
            prev = s;
        }
    }

    #[test]
    fn languages() {
        let q = &TAP[0];
        assert_eq!(in_lang(q, "pt-BR"), "Ei! Isso faz cócegas.");
        assert_eq!(in_lang(q, "es"), "¡Oye! Eso hace cosquillas.");
        assert_eq!(in_lang(q, "en"), "Hey! That tickles.");
        assert_eq!(in_lang(q, ""), "Hey! That tickles.");
    }
}
