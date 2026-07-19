import { Nav } from '@/components/Nav';
import { Footer } from '@/components/Footer';
import { Logo } from '@/components/Logo';
import { WaitlistForm } from '@/components/WaitlistForm';
import { findByRefCode } from '@/db/queries';
import { countQualifiedReferrals } from '@/lib/service';
import { currentTier, isValidRefCode, maskEmail } from '@/lib/referral';

export const dynamic = 'force-dynamic';

async function inviteContext(ref?: string) {
  if (!ref || !isValidRefCode(ref)) return null;
  try {
    const inviter = await findByRefCode(ref);
    if (!inviter) return null;
    const count = await countQualifiedReferrals(ref);
    return { inviter: maskEmail(inviter.email), tier: currentTier(count) };
  } catch {
    return null; // DB not ready — degrade gracefully
  }
}

export default async function Home({
  searchParams,
}: {
  searchParams: Promise<{ ref?: string }>;
}) {
  const { ref } = await searchParams;
  const invite = await inviteContext(ref);

  return (
    <>
      <Nav />
      <main id="top">
        {/* HERO */}
        <section className="hero">
          <div className="hero__sky" aria-hidden="true">
            <div className="cloud cloud--1" />
            <div className="cloud cloud--2" />
            <div className="cloud cloud--3" />
          </div>
          <div className="container hero__inner">
            {invite ? (
              <span className="chip invite-banner">
                <span className="dot" />
                {invite.inviter} invited you{invite.tier ? ` · they’re on ${invite.tier.label}` : ''}
              </span>
            ) : (
              <span className="chip"><span className="dot" />Now in private beta</span>
            )}
            <h1 className="t-display hero__title">
              The AI that remembers your day — and <span className="accent">acts on it</span>.
            </h1>
            <p className="t-body-lg hero__sub muted">
              ShogunAI quietly captures how you work, builds a private memory of your day, and turns
              it into action. Less busywork, more momentum.
            </p>
            <WaitlistForm refCode={ref} />
            <p className="t-body-sm muted hero__note">No credit card required · macOS · Private by default</p>

            {/* Browser mockup */}
            <div className="mock" role="img" aria-label="ShogunAI application preview">
              <div className="mock__bar">
                <span className="mock__dot" /><span className="mock__dot" /><span className="mock__dot" />
                <div className="mock__url"><span className="mock__lock">◈</span> app.shogunai.com</div>
              </div>
              <div className="mock__body">
                <aside className="mock__side">
                  <div className="mock__brand"><Logo size={18} /> ShogunAI</div>
                  <div className="mock__nav mock__nav--active">Today</div>
                  <div className="mock__nav">Memory</div>
                  <div className="mock__nav">Actions</div>
                  <div className="mock__nav">Connections</div>
                  <div className="mock__spacer" />
                  <div className="mock__nav mock__nav--muted">Settings</div>
                </aside>
                <div className="mock__main">
                  <div className="mock__mainhead">
                    <div>
                      <div className="mock__eyebrow">TODAY · THURSDAY</div>
                      <div className="mock__h">Your day, remembered</div>
                    </div>
                    <span className="chip"><span className="dot" />Live capture</span>
                  </div>
                  <div className="mock__grid">
                    <div className="mock__tile"><div className="mock__tile-k">Captured</div><div className="mock__tile-v">2,481</div><div className="mock__tile-s muted">events today</div></div>
                    <div className="mock__tile"><div className="mock__tile-k">Recalled</div><div className="mock__tile-v">14</div><div className="mock__tile-s muted">threads</div></div>
                    <div className="mock__tile mock__tile--accent"><div className="mock__tile-k">Acted on</div><div className="mock__tile-v">9</div><div className="mock__tile-s">tasks done for you</div></div>
                  </div>
                  <div className="mock__row"><span className="mock__pill">Drafted follow-up to Mika</span><span className="mock__time">2m ago</span></div>
                  <div className="mock__row"><span className="mock__pill">Summarized the design review</span><span className="mock__time">18m ago</span></div>
                  <div className="mock__row"><span className="mock__pill">Filed the invoice from Stripe</span><span className="mock__time">1h ago</span></div>
                </div>
              </div>
            </div>
          </div>
        </section>

        {/* TRUST */}
        <section className="trust">
          <div className="container">
            <p className="t-label-sm muted trust__label">Built for people who move fast</p>
            <div className="trust__row">
              <span className="trust__logo">Founders</span>
              <span className="trust__logo">Builders</span>
              <span className="trust__logo">Researchers</span>
              <span className="trust__logo">Operators</span>
              <span className="trust__logo">Creators</span>
            </div>
          </div>
        </section>

        {/* MEMORY */}
        <section className="section" id="memory">
          <div className="container split">
            <div className="split__copy">
              <span className="eyebrow">Memory layer</span>
              <h2 className="t-h1">A memory that captures your day — quietly.</h2>
              <p className="t-body-lg muted">
                ShogunAI builds context passively as you work. No manual note-taking, no constant
                screenshots. Just a searchable, private timeline of what you did, read, and decided —
                ready the moment you need it.
              </p>
              <ul className="ticks">
                <li>Passive capture that stays out of your way</li>
                <li>Hybrid search across everything you touched</li>
                <li>Stored locally — your day never leaves your machine</li>
              </ul>
              <a href="#how" className="btn btn-tertiary">How capture works →</a>
            </div>
            <div className="split__visual">
              <div className="card feature-card">
                <div className="feature-card__head"><span className="chip"><span className="dot" />Recall</span><span className="t-body-sm muted">0.2s</span></div>
                <div className="recall">
                  <div className="recall__q">&quot;What did I decide about pricing on Tuesday?&quot;</div>
                  <div className="recall__a">
                    <div className="recall__line"><b>Tue 14:20</b> — Settled on $49/mo annual, $62 monthly.</div>
                    <div className="recall__line"><b>Tue 14:22</b> — Kept the 7-day trial. Removed the team tier.</div>
                    <div className="recall__src">3 sources · design doc, Slack, notes</div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>

        {/* ACTION */}
        <section className="section band" id="action">
          <div className="container split split--reverse">
            <div className="split__copy">
              <span className="eyebrow">Execution layer</span>
              <h2 className="t-h1">Memory is nice. Action is better.</h2>
              <p className="t-body-lg muted">
                ShogunAI doesn&apos;t stop at remembering. It uses your context to actually get things
                done — drafting the reply, filing the doc, prepping the meeting — across the tools you
                already use.
              </p>
              <ul className="ticks">
                <li>Turns memory into concrete next steps</li>
                <li>Connects to 20+ tools out of the box</li>
                <li>You stay in control — approve before it acts</li>
              </ul>
              <a href="#get-started" className="btn btn-primary">Try it free</a>
            </div>
            <div className="split__visual">
              <div className="action-flow">
                <div className="card action-step"><div className="action-step__k">Notices</div><div className="action-step__v">You mentioned sending Mika the deck.</div></div>
                <div className="action-arrow">↓</div>
                <div className="card action-step action-step--accent"><div className="action-step__k">Acts</div><div className="action-step__v">Drafted the email, attached v3, waiting for your OK.</div></div>
                <div className="action-arrow">↓</div>
                <div className="card action-step"><div className="action-step__k">Confirms</div><div className="action-step__v">Sent. Logged to memory. Nothing left open.</div></div>
              </div>
            </div>
          </div>
        </section>

        {/* HOW */}
        <section className="section" id="how">
          <div className="container">
            <div className="section-head">
              <span className="eyebrow">How it works</span>
              <h2 className="t-h1">Three steps. Then it fades into the background.</h2>
            </div>
            <div className="steps">
              <div className="card step"><div className="step__num">01</div><h3 className="t-h3">Capture</h3><p className="t-body muted">ShogunAI observes your day passively and builds a private, local memory — no screenshots, no manual logging.</p></div>
              <div className="card step"><div className="step__num">02</div><h3 className="t-h3">Recall</h3><p className="t-body muted">Ask anything about your work in plain language and get an answer grounded in what actually happened.</p></div>
              <div className="card step"><div className="step__num">03</div><h3 className="t-h3">Act</h3><p className="t-body muted">Turn that context into finished work across your tools — with your approval on anything that matters.</p></div>
            </div>
          </div>
        </section>

        {/* STATS */}
        <section className="section stats-band">
          <div className="container stats">
            <div className="stat"><div className="t-display stat__v">4h</div><div className="t-body muted">saved per week, on average</div></div>
            <div className="stat"><div className="t-display stat__v">20+</div><div className="t-body muted">tools connected out of the box</div></div>
            <div className="stat"><div className="t-display stat__v">100%</div><div className="t-body muted">of your memory stays local</div></div>
          </div>
        </section>

        {/* PRICING */}
        <section className="section" id="pricing">
          <div className="container">
            <div className="section-head">
              <span className="eyebrow">Pricing</span>
              <h2 className="t-h1">Simple, honest pricing.</h2>
              <p className="t-body-lg muted">Start free. Upgrade when ShogunAI is running your day.</p>
            </div>
            <div className="pricing">
              <div className="card price">
                <div className="price__name">Free</div>
                <div className="price__amt"><span className="price__num">$0</span><span className="muted">/mo</span></div>
                <p className="t-body muted">For trying ShogunAI on a single machine.</p>
                <ul className="ticks ticks--sm"><li>Passive memory capture</li><li>Natural-language recall</li><li>3 connected tools</li></ul>
                <a href="#get-started" className="btn btn-secondary price__btn">Get started</a>
              </div>
              <div className="card price price--featured">
                <span className="chip price__badge"><span className="dot" />Most popular</span>
                <div className="price__name">Pro</div>
                <div className="price__amt"><span className="price__num">$49</span><span className="muted">/mo, billed annually</span></div>
                <p className="t-body muted">$62 monthly. 7-day free trial.</p>
                <ul className="ticks ticks--sm"><li>Everything in Free</li><li>Unlimited memory &amp; recall</li><li>20+ tools &amp; autonomous actions</li><li>Priority support</li></ul>
                <a href="#get-started" className="btn btn-primary price__btn">Start 7-day trial</a>
              </div>
            </div>
          </div>
        </section>

        {/* CTA / waitlist */}
        <section className="section cta" id="get-started">
          <div className="container">
            <div className="cta__card">
              <div className="cta__sky" aria-hidden="true" />
              <div className="cta__content">
                <h2 className="t-h1">Give your AI a memory. Let it act.</h2>
                <p className="t-body-lg muted">Join the private beta and put ShogunAI to work on your day.</p>
                <WaitlistForm refCode={ref} />
                <p className="t-body-sm muted" style={{ marginTop: 14 }}>Backed by builders. Private by default.</p>
              </div>
            </div>
          </div>
        </section>
      </main>
      <Footer />
    </>
  );
}
