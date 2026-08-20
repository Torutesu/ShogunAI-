import type { Locale } from '@/i18n/config';
import { localizedMarketingContent } from './marketing-content-locales';

export type MarketingDetail = {
  slug: string;
  eyebrow: string;
  title: string;
  description: string;
  intro: string;
  highlights: readonly { title: string; body: string }[];
  steps: readonly { title: string; body: string }[];
  outcomes: readonly string[];
  faq: readonly (readonly [string, string])[];
};

export const featurePages: readonly MarketingDetail[] = [
  {
    slug: 'ai-memory',
    eyebrow: 'AI memory',
    title: 'A private memory for the work behind your work',
    description: 'ShogunAI captures work context on your Mac and turns it into a searchable, structured timeline—without relying on screenshots.',
    intro: 'The useful part of work is rarely in one document. It lives across conversations, browser tabs, decisions, and half-finished drafts. ShogunAI gives that context a durable home so you can return to it later without rebuilding the story from scratch.',
    highlights: [
      { title: 'Local-first by default', body: 'Your work memory is designed to remain on your device by default. You choose when a connected provider receives relevant context.' },
      { title: 'Context, not a screenshot archive', body: 'ShogunAI is built around structured, searchable work context rather than a folder of images you must inspect manually. Optional visual recall adds screenshots where text capture fails: opt-in, local, kept for a retention period you choose, and deleted automatically at that age.' },
      { title: 'Passive capture', body: 'Build useful memory while you work, without turning every decision into a note-taking task.' },
    ],
    steps: [
      { title: 'Capture', body: 'ShogunAI observes the work context you allow on macOS and organizes it locally.' },
      { title: 'Connect', body: 'Related moments, people, projects, and tools become part of one searchable history.' },
      { title: 'Control', body: 'Pause capture, choose connected services, and delete local memory when you need to.' },
    ],
    outcomes: ['Remember why a decision was made', 'Find the source behind a detail', 'Resume interrupted work faster', 'Keep sensitive context local by default'],
    faq: [
      ['Does ShogunAI store screenshots?', 'Not by default: capture reads text through the macOS accessibility layer, and no image is written. The exception is visual recall, which you turn on yourself. With it on, and only where a window yields no text at all, a compressed screenshot is kept in the encrypted local database for the retention period you select — presets from one to seven days, plus a bounded custom duration, with three days as the default — then deleted automatically at that age. You can view or delete saved frames at any time during the window. Frames never leave your Mac.'],
      ['Where is memory stored?', 'Memory is local-first and remains on your Mac by default. Optional connected services may receive only the context required for the action you approve.'],
      ['Do I need to write notes manually?', 'No. Passive capture is designed to reduce manual note-taking, while still letting you add or remove context intentionally.'],
    ],
  },
  {
    slug: 'contextual-recall',
    eyebrow: 'Contextual recall',
    title: 'Ask about your work without pasting the backstory',
    description: 'Recall decisions, conversations, documents, and project context in natural language with answers grounded in your own work history.',
    intro: 'General chatbots begin with an empty conversation. ShogunAI begins with the work context you have already built, helping you answer questions that normally require searching several apps and reconstructing what happened.',
    highlights: [
      { title: 'Natural-language search', body: 'Ask the way you remember: by person, project, decision, date, or intent—not only by exact keyword.' },
      { title: 'Cross-tool context', body: 'Connect related information that would otherwise stay fragmented across messages, mail, documents, and browser research.' },
      { title: 'Grounded answers', body: 'Use your own work history as the starting point instead of receiving a generic answer with no awareness of your situation.' },
    ],
    steps: [
      { title: 'Ask', body: 'Describe what you need in ordinary language, even if you only remember part of it.' },
      { title: 'Retrieve', body: 'ShogunAI identifies the most relevant context from your private work memory.' },
      { title: 'Continue', body: 'Turn the answer into a brief, reply, plan, or next action without starting over.' },
    ],
    outcomes: ['Search less across Slack, Gmail, and docs', 'Prepare for meetings with the full context', 'Recover decisions after weeks away', 'Draft from facts you already know'],
    faq: [
      ['How is this different from ordinary search?', 'Ordinary search expects exact words and separate app searches. Contextual recall is designed to retrieve related work by meaning, people, projects, and decisions.'],
      ['Can it answer questions across multiple tools?', 'ShogunAI is designed to combine local work memory with the connected tools you authorize, creating one context layer across your workflow.'],
      ['Can I use my preferred AI provider?', 'Yes. ShogunAI supports a bring-your-own-key model so you can choose a supported AI provider and control that relationship directly.'],
    ],
  },
  {
    slug: 'execution-layer',
    eyebrow: 'Execution layer',
    title: 'Turn remembered context into finished work',
    description: 'Move from recall to action across connected tools, with approval gates for anything consequential.',
    intro: 'Memory is only valuable when it reduces work. ShogunAI uses the context behind a request to prepare the next step—drafting a response, organizing a document, or preparing a meeting—then keeps you in control of consequential actions.',
    highlights: [
      { title: 'Context-aware drafts', body: 'Start with the relevant project history, decisions, and preferences already in your memory.' },
      { title: '20+ connected tools', body: 'Work across the tools you already use instead of moving every task into another standalone workspace.' },
      { title: 'Approval gates', body: 'Review actions that send, publish, modify, or otherwise matter before they are completed.' },
    ],
    steps: [
      { title: 'Understand', body: 'ShogunAI retrieves the context required to interpret your request accurately.' },
      { title: 'Prepare', body: 'The execution layer creates the draft, plan, update, or tool action.' },
      { title: 'Approve', body: 'You review consequential actions before ShogunAI completes them.' },
    ],
    outcomes: ['Draft follow-ups after meetings', 'Prepare project and investor updates', 'File and organize work consistently', 'Move from question to completed next step'],
    faq: [
      ['Can ShogunAI act without my approval?', 'Consequential actions are designed to use approval gates. You remain responsible for what is sent, changed, or published.'],
      ['What kinds of work can it help with?', 'Examples include drafting replies, preparing meetings, organizing documents, creating briefs, and carrying context into connected tools.'],
      ['Which plan includes execution?', 'Standard includes everyday execution with core tool connections. Pro adds unlimited memory and recall, every supported tool, and autonomous execution.'],
    ],
  },
] as const;

export const useCasePages: readonly MarketingDetail[] = [
  {
    slug: 'founders',
    eyebrow: 'For founders',
    title: 'Keep the company context you cannot afford to lose',
    description: 'ShogunAI helps founders recall decisions, prepare investor and team updates, and turn scattered company context into action.',
    intro: 'A founder moves between product, hiring, customers, fundraising, and operations every day. ShogunAI keeps the context behind those switches available, so the next conversation starts with what already happened.',
    highlights: [
      { title: 'Investor preparation', body: 'Bring prior updates, metrics discussions, open questions, and commitments into one preparation flow.' },
      { title: 'Decision history', body: 'Recover why the team chose a direction—not only the final task or document.' },
      { title: 'Follow-through', body: 'Turn meetings and conversations into drafts, updates, and clearly owned next actions.' },
    ],
    steps: [
      { title: 'Capture the operating context', body: 'Build memory across the projects, conversations, and research that shape the company.' },
      { title: 'Ask before the next decision', body: 'Recall previous commitments, objections, and assumptions in natural language.' },
      { title: 'Ship the follow-up', body: 'Prepare the update or response from the same context and approve the final action.' },
    ],
    outcomes: ['Prepare board and investor updates', 'Resume fundraising conversations accurately', 'Keep hiring context across interviews', 'Reduce founder context switching'],
    faq: [
      ['Is ShogunAI a company knowledge base?', 'ShogunAI is primarily a private memory and execution layer for the individual. It complements shared company knowledge by preserving the context behind your own work.'],
      ['Can it help with investor meetings?', 'Yes. It can help retrieve prior discussions, questions, commitments, and related work to prepare a grounded briefing.'],
      ['Does my team need to change tools?', 'No. ShogunAI is designed to work across existing tools rather than require a complete workflow migration.'],
    ],
  },
  {
    slug: 'product-engineering',
    eyebrow: 'For product & engineering',
    title: 'Carry product context from discussion to delivery',
    description: 'Recall technical decisions, customer evidence, design trade-offs, and project history without searching every tool separately.',
    intro: 'Product work loses time at the seams: a customer request in email, a decision in Slack, a design in Figma, and an issue in Linear. ShogunAI creates a personal context layer across those moments.',
    highlights: [
      { title: 'Decision recall', body: 'Find the reasoning, alternatives, and constraints behind a product or technical choice.' },
      { title: 'Faster handoffs', body: 'Prepare concise context for teammates without manually reconstructing every source.' },
      { title: 'Project continuity', body: 'Return to interrupted work with the relevant history and next steps already available.' },
    ],
    steps: [
      { title: 'Follow the work', body: 'Capture relevant context as work moves between research, discussion, design, and implementation.' },
      { title: 'Retrieve the why', body: 'Ask about a feature, bug, customer, or decision using the language your team already uses.' },
      { title: 'Create the artifact', body: 'Turn that context into a brief, issue, update, or handoff for review.' },
    ],
    outcomes: ['Write better product briefs', 'Recover technical rationale', 'Prepare sprint and launch updates', 'Onboard yourself back into old projects'],
    faq: [
      ['Does this replace Linear, Jira, or Notion?', 'No. ShogunAI is a context and execution layer across your existing tools, not a replacement project-management system.'],
      ['Can it connect code and product context?', 'It is designed to relate work across the tools you authorize, helping you recall the discussions and artifacts surrounding implementation.'],
      ['Is it useful for individual contributors?', 'Yes. The product is designed around an individual’s private work memory, including engineers, designers, and product managers.'],
    ],
  },
  {
    slug: 'consultants',
    eyebrow: 'For consultants',
    title: 'Remember every client without carrying every detail in your head',
    description: 'Keep client-specific context separate, prepare faster, and create follow-ups grounded in the work already completed.',
    intro: 'Client work demands rapid switching between companies, people, terminology, and commitments. ShogunAI helps you recover the right context before a call and turn it into deliverables afterward.',
    highlights: [
      { title: 'Client recall', body: 'Retrieve prior conversations, constraints, deliverables, and unresolved questions by client or project.' },
      { title: 'Meeting preparation', body: 'Create a focused briefing from recent work instead of scanning every message and document.' },
      { title: 'Consistent follow-up', body: 'Draft summaries and next steps grounded in what was actually discussed.' },
    ],
    steps: [
      { title: 'Build private client memory', body: 'Capture the context you need while keeping your personal work layer local-first.' },
      { title: 'Prepare before the call', body: 'Recall recent changes, commitments, and open decisions in one query.' },
      { title: 'Deliver after the call', body: 'Create a clear recap, plan, or client-ready draft and approve it before sending.' },
    ],
    outcomes: ['Switch clients with less mental overhead', 'Create better meeting briefs', 'Reduce missed commitments', 'Draft recaps and proposals faster'],
    faq: [
      ['Can I keep different client contexts separate?', 'ShogunAI is designed around controlled, searchable work context. You decide what is captured and which connected services are authorized.'],
      ['Will it send client emails automatically?', 'Consequential actions use approval gates, so you can review a client-facing message before it is sent.'],
      ['Is the memory cloud-only?', 'No. ShogunAI is local-first by default, with optional provider sharing only where an enabled feature requires it.'],
    ],
  },
] as const;

export function findMarketingPage(pages: readonly MarketingDetail[], slug: string) {
  return pages.find((page) => page.slug === slug);
}

export function getFeaturePages(locale: Locale = 'en'): readonly MarketingDetail[] {
  return locale === 'en' ? featurePages : localizedMarketingContent[locale].features;
}

export function getUseCasePages(locale: Locale = 'en'): readonly MarketingDetail[] {
  return locale === 'en' ? useCasePages : localizedMarketingContent[locale].useCases;
}
