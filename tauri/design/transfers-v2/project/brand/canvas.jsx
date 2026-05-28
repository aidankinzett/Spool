/* Top-level canvas: header section + 5 brand-direction sections.
   Each direction has 3 artboards (Identity, Marks, In-context). */

function BrandCanvasApp() {
  return (
    <DesignCanvas>
      <DCSection id="brief" title="Brief" subtitle="Shared system + voice notes">
        <DCArtboard id="intro" label="The ask" width={780} height={420}>
          <IntroCard />
        </DCArtboard>
      </DCSection>

      {DIRECTIONS.map((dir, i) => (
        <DCSection
          key={dir.id}
          id={dir.id}
          title={`${String(i + 1).padStart(2, "0")} · ${dir.name}`}
          subtitle={dir.tagline}
        >
          <DCArtboard id={`${dir.id}-id`} label="Identity" width={560} height={500}>
            <IdentityCard dir={dir} />
          </DCArtboard>
          <DCArtboard id={`${dir.id}-marks`} label="Mark studies" width={620} height={500}>
            <MarksCard dir={dir} />
          </DCArtboard>
          <DCArtboard id={`${dir.id}-context`} label="In context" width={620} height={500}>
            <InContextCard dir={dir} />
          </DCArtboard>
        </DCSection>
      ))}

      <DCSection
        id="spool-alternates"
        title="Spool · alternate marks"
        subtitle="Eight directions on the same idea — find the right one"
      >
        <DCArtboard id="spool-grid" label="The eight" width={780} height={560}>
          <SpoolGridCard />
        </DCArtboard>
        <DCArtboard id="spool-tiny" label="Tiny-size test" width={620} height={560}>
          <SpoolTinyCard />
        </DCArtboard>
        <DCArtboard id="spool-accent" label="Accent contexts" width={620} height={560}>
          <SpoolAccentCard />
        </DCArtboard>
      </DCSection>

      <DCSection
        id="reel-refinements"
        title="Reel-to-reel & Cassette · refinements"
        subtitle="The tape leaves the bottom of each reel — fixing that, plus cassette variations"
      >
        <DCArtboard id="reel-grid" label="Ten takes" width={860} height={560}>
          <ReelGridCard />
        </DCArtboard>
        <DCArtboard id="reel-tiny" label="Tiny-size test" width={620} height={620}>
          <ReelTinyCard />
        </DCArtboard>
        <DCArtboard id="reel-lockup" label="Lockup studies" width={520} height={620}>
          <ReelLockupCard />
        </DCArtboard>
      </DCSection>
    </DesignCanvas>
  );
}

ReactDOM.createRoot(document.getElementById("root")).render(<BrandCanvasApp />);
