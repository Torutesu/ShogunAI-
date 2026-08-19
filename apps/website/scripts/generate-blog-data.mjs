import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import matter from 'gray-matter';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import * as runtime from 'react/jsx-runtime';

const require = createRequire(import.meta.url);
const nextMdxRemoteDir = dirname(require.resolve('next-mdx-remote/package.json'));
const mdxModulePath = require.resolve('@mdx-js/mdx', { paths: [nextMdxRemoteDir] });
const { evaluate } = await import(pathToFileURL(mdxModulePath).href);

const BLOG_DIR = join(process.cwd(), 'content', 'blog');
const OUTPUT_FILE = join(process.cwd(), 'src', 'lib', 'blog-data.generated.ts');
const LOCALES = new Set(['en', 'ja', 'es', 'de']);
// Two tags only: Ideas (the thinking) and Product (how it works).
const CATEGORY_IMAGES = {
  Ideas: '/images/blog/ai-memory.png',
  Product: '/images/blog/product.png',
};

// Every published article gets a real body for every supported locale. These
// editorial summaries are intentionally source-controlled so a language
// switch never silently renders English content under a translated URL.
const LOCALIZED_ARTICLE_COPY = {
  ja: {
    'ai-memory-context-layer-guide': ['AIに記憶が必要な理由', 'AIメモリとコンテキストレイヤー、プライバシー、実行までを実務目線で解説します。', ['AIモデルが賢くなっても、仕事の背景を毎回説明し直す必要があれば実務では十分に役立ちません。メモリは過去の決定や好みを保持し、コンテキストは今のタスクに必要な情報を組み立てます。', '有用なレイヤーには、選択的な取得、検索可能な保存、根拠の提示、明示的な権限、削除と一時停止の操作が必要です。', 'ShogunAIはローカルファーストの記憶と、承認を挟んだ実行を一つの個人向けレイヤーとして設計しています。']],
    'how-to-search-slack-gmail-notion': ['Slack・Gmail・Notionを横断して検索する方法', '分散した決定と文脈を実務で見つけるためのワークフローです。', ['仕事の文脈は一か所に保存されません。決定はSlackで始まり、Gmailで補足され、Notionにまとまることがあります。', 'アプリ名ではなく、決定、担当、次の行動を質問にすると、関連情報を時系列で再構成できます。', '検索結果には出典を付け、送信や変更は承認を挟みます。']],
    'investors-on-the-memory-and-context-layer': ['メモリとコンテキストレイヤーをめぐる投資家の見方', 'AIエージェントにおける文脈、記憶、同僚のような実行を一次情報に基づいて整理します。', ['AIの競争はモデルの大きさだけでなく、エージェントが仕事の背景を理解できるかへ移っています。', '文脈レイヤーはデータ、関係性、権限、暗黙知を扱い、メモリレイヤーは必要な情報を時間をまたいで保持します。', 'ShogunAIは、日々の記憶と承認された実行を個人の仕事に接続する位置にあります。']],
    'passive-memory-not-screenshots': ['スクリーンショットではなく、受動的な記憶', '監視フィードにせず、仕事の文脈をプライベートに取り戻す方法を解説します。', ['スクリーンショットは文脈の粗い代理です。何が重要だったか、何が変わったか、次に何をすべきかまでは説明しません。', '有用な受動的記憶は、端末内を基本に、検索、削除、一時停止、権限の境界を明確にします。']],
    'the-execution-layer': ['実行レイヤーとは何か', '記憶した文脈を、承認可能な仕事の次の一歩へ変える仕組みです。', ['メモリは、次の仕事を減らして初めて価値になります。ShogunAIは決定や好みを取得し、下書き、計画、更新、ツール操作を準備します。', '送信、公開、変更、削除などの重要操作は、根拠を確認してから承認できるようにします。']],
    'welcome-to-shogunai': ['ShogunAIへようこそ', '一日の記憶と、それに基づく実行を一つの個人向けOSとして考える理由です。', ['仕事の価値ある文脈は、一つの文書ではなく会話、ブラウザ、決定、途中の下書きに分散しています。', 'ShogunAIはその背景を取り戻し、次の一歩を準備します。ローカル優先、明示的なプロバイダ選択、重要操作の承認を原則にします。']],
    'why-local-first': ['なぜローカルファーストなのか', '仕事の記憶を自分の端末に置く意味と、そこから生まれる選択肢を説明します。', ['個人のメモリには、決定、人間関係、資料、未完成の考えが含まれます。必要のないクラウドコピーを作らないことが重要です。', '端末内の保存を基本にすると、削除が意味を持ち、AIプロバイダへ共有するタイミングを自分で選べます。']],
  },
  es: {
    'ai-memory-context-layer-guide': ['Por qué la IA necesita memoria', 'Una guía práctica sobre memoria, contexto, privacidad y ejecución para el trabajo diario.', ['Un modelo inteligente sigue empezando de cero si no conoce el contexto de tu trabajo. La memoria conserva decisiones y preferencias; el contexto reúne lo necesario para la tarea actual.', 'Una capa útil necesita captura selectiva, búsqueda con fuentes, permisos claros y controles de pausa y borrado.', 'ShogunAI une memoria local-first y ejecución con aprobación para el individuo.']],
    'bring-your-plan-not-just-your-keys': ['Trae tu plan, no solo tus claves', 'Pro ya no exige una clave de API: el trabajo del agente puede ejecutarse dentro de la suscripción de asistente que ya pagas.', ['Pedir una clave de API significaba una segunda relación de facturación para usar un modelo que ya pagas cada mes. Ahora Pro funciona con la suscripción de asistente que ya tienes, y la clave pasa a ser opcional.', 'ShogunAI ejecuta la herramienta oficial del proveedor bajo sus propios términos. No leemos los almacenes de credenciales de otras aplicaciones ni reimplementamos su inicio de sesión.', 'El trabajo por lotes (indexación, clasificación, revisión nocturna y resumen matinal) sigue en nuestra infraestructura, para no agotar tu cuota mensual y dejar al agente sin margen.']],
    'how-to-search-slack-gmail-notion': ['Cómo buscar en Slack, Gmail y Notion', 'Un flujo para reconstruir decisiones y contexto repartidos entre tus herramientas.', ['El trabajo rara vez vive en un solo lugar. Una decisión puede empezar en Slack, aclararse por Gmail y terminar en Notion.', 'Pregunta por la decisión, la persona responsable y el siguiente paso, no solo por el nombre de la aplicación.', 'Las respuestas deben incluir fuentes y pedir aprobación antes de enviar o cambiar algo.']],
    'investors-on-the-memory-and-context-layer': ['Qué dicen los inversores sobre memoria y contexto', 'Una síntesis sobre contexto, memoria y agentes que trabajan como compañeros.', ['La ventaja de la IA no depende solo de modelos más grandes, sino de entender el contexto de cada trabajo.', 'La capa de contexto conecta entidades, relaciones, permisos y conocimiento tácito; la memoria conserva lo que seguirá siendo útil.', 'ShogunAI conecta memoria personal y ejecución aprobada.']],
    'meeting-notes-without-the-recording': ['Notas de reunión sin grabación', 'Las reuniones pasan a formar parte de tu memoria en lugar de una carpeta de archivos de audio, y se incluyen en todos los planes.', ['ShogunAI detecta que estás en una reunión y genera la transcripción y el resumen sin que ningún bot se una a la llamada.', 'Se guarda el texto de la transcripción, quién habló y de qué reunión procede. El audio no: ShogunAI no escribe grabaciones ni archivos temporales en disco en ningún momento.', 'La transcripción en vivo pasa por un servicio de voz y lo decimos de forma explícita: el audio solo se usa para producir el texto, nunca para entrenar modelos, y la función puede desactivarse sin afectar al resto.']],
    'passive-memory-not-screenshots': ['Memoria pasiva, no capturas', 'Cómo recuperar contexto privado sin convertir tu pantalla en un sistema de vigilancia.', ['Las capturas son un sustituto pobre del contexto: no explican qué importó ni qué debe ocurrir después.', 'La memoria pasiva debe ser selectiva, local-first, buscable y fácil de pausar, borrar y controlar.']],
    'the-execution-layer': ['La capa de ejecución', 'Cómo la memoria se convierte en trabajo terminado con aprobación humana.', ['La memoria vale cuando reduce el trabajo posterior. ShogunAI prepara borradores, planes y acciones en las herramientas que ya usas.', 'Enviar, publicar, cambiar o borrar requiere una revisión visible antes de producir consecuencias.']],
    'visual-recall': ['Recuerdo visual y la excepción que discutimos', 'El caso concreto en el que el texto no basta, y las reglas que pusimos antes de guardar un solo fotograma.', ['ShogunAI lee texto, no píxeles. Pero un gráfico compartido en pantalla o un PDF escaneado no ofrecen texto, y ahí la memoria se queda en silencio.', 'El recuerdo visual está desactivado por defecto. Al activarlo, y solo cuando la extracción de texto falla, se guarda un fotograma comprimido en la base de datos cifrada durante el plazo finito que eliges y luego se borra solo.', 'Nada se sube: los fotogramas se leen en tu Mac. La línea de tiempo permanece local, el audio queda fuera de esta excepción y, con la opción desactivada, no se escribe ninguna imagen.']],
    'welcome-to-shogunai': ['Te damos la bienvenida a ShogunAI', 'Por qué construimos un sistema operativo para la persona AI-native.', ['El contexto importante vive entre conversaciones, pestañas, decisiones y borradores. ShogunAI lo hace recuperable sin reconstruir la historia.', 'La privacidad local-first, la elección del proveedor y la aprobación de acciones son principios del producto.']],
    'why-local-first': ['Por qué local-first', 'Qué significa mantener la memoria de tu trabajo en tu propio dispositivo.', ['La memoria personal contiene decisiones, relaciones y documentos que no deberían convertirse en una copia permanente en la nube.', 'El almacenamiento local reduce copias innecesarias y deja en tus manos cuándo compartir el contexto con un proveedor de IA.']],
  },
  de: {
    'ai-memory-context-layer-guide': ['Warum KI Gedächtnis braucht', 'Ein praktischer Leitfaden zu Gedächtnis, Kontext, Datenschutz und Ausführung.', ['Ein kluges Modell beginnt trotzdem bei null, wenn es den Arbeitskontext nicht kennt. Gedächtnis bewahrt Entscheidungen und Präferenzen; Kontext stellt die Informationen für die aktuelle Aufgabe zusammen.', 'Eine gute Schicht braucht selektive Erfassung, Quellen, klare Berechtigungen und Pausieren- sowie Löschkontrollen.', 'ShogunAI verbindet local-first Gedächtnis mit Ausführung und Freigabe.']],
    'bring-your-plan-not-just-your-keys': ['Bring deinen Plan mit, nicht nur deine Schlüssel', 'Pro verlangt keinen API-Schlüssel mehr: Die Agentenarbeit läuft im Assistenz-Abo, das du ohnehin bezahlst.', ['Ein API-Schlüssel bedeutete eine zweite Abrechnungsbeziehung für ein Modell, das du bereits monatlich bezahlst. Pro läuft jetzt auf dem vorhandenen Assistenz-Abo, der Schlüssel bleibt optional.', 'ShogunAI startet das offizielle Werkzeug des Anbieters unter dessen Bedingungen. Wir lesen keine Zugangsdaten anderer Anwendungen aus und bauen keine fremde Anmeldung nach.', 'Die Stapelarbeit (Indizierung, Klassifizierung, nächtliche Auswertung, Morgenbriefing) bleibt auf unserer Infrastruktur, damit dein monatliches Kontingent nicht in Tagen aufgebraucht ist.']],
    'how-to-search-slack-gmail-notion': ['Slack, Gmail und Notion gemeinsam durchsuchen', 'Ein Workflow für verteilte Entscheidungen und Arbeitskontext.', ['Arbeit liegt selten an einem Ort. Eine Entscheidung kann in Slack beginnen, per Gmail geklärt und in Notion dokumentiert werden.', 'Frage nach Entscheidung, Verantwortlichen und nächstem Schritt statt nur nach dem App-Namen.', 'Ergebnisse brauchen Quellen; Änderungen und Nachrichten warten auf deine Freigabe.']],
    'investors-on-the-memory-and-context-layer': ['Was Investoren über Gedächtnis und Kontext sagen', 'Ein Überblick über Kontext, Gedächtnis und Agenten als Kollegen.', ['Der Wettbewerb der KI hängt nicht nur von größeren Modellen ab, sondern davon, ob sie Arbeitskontext verstehen.', 'Die Kontextschicht verbindet Entitäten, Beziehungen, Berechtigungen und implizites Wissen; Gedächtnis bewahrt langfristig nützliche Informationen.', 'ShogunAI verbindet persönliches Gedächtnis mit freigegebener Ausführung.']],
    'meeting-notes-without-the-recording': ['Besprechungsnotizen ohne Aufnahme', 'Meetings werden Teil deines Gedächtnisses statt eines Ordners voller Audiodateien — enthalten in jedem Plan.', ['ShogunAI erkennt, dass du in einem Meeting bist, und erstellt Transkript und Zusammenfassung, ohne dass ein Bot dem Anruf beitritt.', 'Gespeichert werden Transkripttext, Sprecher und Zuordnung zum Meeting. Das Audio nicht: ShogunAI schreibt zu keinem Zeitpunkt Aufnahmen oder temporäre Audiodateien auf die Festplatte.', 'Die Live-Transkription läuft über einen Sprachdienst, und wir sagen das ausdrücklich: Das Audio dient nur der Texterzeugung, nie dem Training von Modellen, und die Funktion lässt sich einzeln abschalten.']],
    'passive-memory-not-screenshots': ['Passives Gedächtnis statt Screenshots', 'Arbeitskontext privat zurückholen, ohne den Bildschirm in einen Überwachungsfeed zu verwandeln.', ['Screenshots sind ein grober Ersatz für Kontext und erklären weder Wichtiges noch den nächsten Schritt.', 'Passives Gedächtnis sollte selektiv, lokal-first, durchsuchbar und leicht pausierbar und löschbar sein.']],
    'the-execution-layer': ['Die Ausführungsschicht', 'Wie Gedächtnis mit menschlicher Freigabe zu fertiger Arbeit wird.', ['Gedächtnis ist wertvoll, wenn es Folgearbeit reduziert. ShogunAI erstellt Entwürfe, Pläne und Aktionen in deinen bestehenden Tools.', 'Senden, Veröffentlichen, Ändern und Löschen bleiben vor der Ausführung überprüfbar.']],
    'visual-recall': ['Visuelle Erinnerung und die Ausnahme, über die wir gestritten haben', 'Der eine Fall, in dem Text nicht reicht — und die Regeln, die wir vor dem ersten gespeicherten Einzelbild festgelegt haben.', ['ShogunAI liest Text, keine Pixel. Ein geteiltes Diagramm oder ein eingescanntes PDF liefern aber keinen Text, und dort bleibt das Gedächtnis stumm.', 'Visuelle Erinnerung ist standardmäßig aus. Eingeschaltet und nur wenn die Textextraktion leer bleibt, wird ein komprimiertes Einzelbild für die von dir gewählte endliche Dauer in der verschlüsselten Datenbank gehalten und danach automatisch gelöscht.', 'Nichts wird hochgeladen: Einzelbilder werden auf deinem Mac gelesen. Es gibt keine Zeitleiste und keine Wiedergabe, Audio ist von dieser Ausnahme ausgenommen, und ausgeschaltet wird kein Bild geschrieben.']],
    'welcome-to-shogunai': ['Willkommen bei ShogunAI', 'Warum wir ein Betriebssystem für den AI-native Einzelnen bauen.', ['Wichtiger Kontext liegt zwischen Gesprächen, Tabs, Entscheidungen und Entwürfen. ShogunAI macht ihn wieder auffindbar.', 'Local-first Datenschutz, Anbieterwahl und Freigabe wichtiger Aktionen sind Produktprinzipien.']],
    'why-local-first': ['Warum local-first', 'Was es bedeutet, dein Arbeitsgedächtnis auf deinem eigenen Gerät zu halten.', ['Persönliches Gedächtnis enthält Entscheidungen, Beziehungen und Dokumente, die nicht dauerhaft in der Cloud liegen sollten.', 'Lokale Speicherung reduziert Kopien und lässt dich selbst entscheiden, wann ein KI-Anbieter Kontext erhält.']],
  },
};

function parseName(nameNoExt) {
  // This legacy file is Japanese content without a locale suffix. Keep its
  // public slug while treating it as the Japanese variant.
  if (nameNoExt === 'ai-memory-context-layer-guide') {
    return { slug: nameNoExt, locale: 'ja' };
  }
  const dot = nameNoExt.lastIndexOf('.');
  if (dot > 0) {
    const maybeLocale = nameNoExt.slice(dot + 1);
    if (LOCALES.has(maybeLocale)) {
      return { slug: nameNoExt.slice(0, dot), locale: maybeLocale };
    }
  }
  return { slug: nameNoExt, locale: 'en' };
}

// CJK text carries no spaces, so a word count reads every Japanese article as
// one minute. Count CJK characters on their own scale (~500/min) and score the
// remaining prose by words (~200/min).
const CJK = /[぀-ヿ㐀-䶿一-鿿豈-﫿]/gu;

function readingMinutes(text) {
  const cjkChars = (text.match(CJK) ?? []).length;
  const rest = text.replace(CJK, ' ').trim();
  const words = rest ? rest.split(/\s+/).length : 0;
  return Math.max(1, Math.round(cjkChars / 500 + words / 200));
}

const entries = [];
const files = readdirSync(BLOG_DIR).filter((file) => file.endsWith('.mdx')).sort();

for (const file of files) {
  const raw = readFileSync(join(BLOG_DIR, file), 'utf8');
  const { data, content } = matter(raw);
  const { slug, locale } = parseName(file.replace(/\.mdx$/, ''));
  const compiled = await evaluate(content, {
    ...runtime,
    baseUrl: import.meta.url,
    format: 'md',
    development: false,
  });

  entries.push({
    slug,
    locale,
    title: String(data.title ?? slug),
    description: String(data.description ?? ''),
    date: String(data.date ?? '1970-01-01'),
    category: String(data.category ?? 'Product'),
    author: String(data.author ?? 'ShogunAI'),
    image: String(data.image ?? CATEGORY_IMAGES[String(data.category ?? 'Product')] ?? CATEGORY_IMAGES.Product),
    readingMinutes: readingMinutes(content),
    html: renderToStaticMarkup(createElement(compiled.default)),
  });
}

// Some articles are authored in MDX only once. Materialize their localized
// editorial copy here so every published slug has a real body for every
// supported locale, including static generation and related-card metadata.
const sourceEntries = [...entries];
const escapeHtml = (value) => String(value)
  .replaceAll('&', '&amp;')
  .replaceAll('<', '&lt;')
  .replaceAll('>', '&gt;')
  .replaceAll('"', '&quot;')
  .replaceAll("'", '&#39;');

for (const base of sourceEntries.filter((entry) => entry.locale === 'en')) {
  for (const locale of ['ja', 'es', 'de']) {
    if (entries.some((entry) => entry.slug === base.slug && entry.locale === locale)) continue;
    const copy = LOCALIZED_ARTICLE_COPY[locale]?.[base.slug];
    if (!copy) continue;
    const paragraphs = copy[2];
    entries.push({
      ...base,
      locale,
      title: copy[0],
      description: copy[1],
      readingMinutes: readingMinutes(paragraphs.join(' ')),
      html: paragraphs.map((paragraph) => `<p>${escapeHtml(paragraph)}</p>`).join('\n'),
    });
  }
}

const source = `/* This file is generated by scripts/generate-blog-data.mjs. */\nexport const BLOG_DATA = ${JSON.stringify(entries, null, 2)} as const;\n`;
writeFileSync(OUTPUT_FILE, source);

console.log(`Generated ${entries.length} localized blog records.`);
