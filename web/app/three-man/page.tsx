import TeamTable from '@/components/TeamTable';

export const metadata = { title: '3-Man Guildford — WCA Stats' };

export default function ThreeManPage() {
  return (
    <>
      <div className="page-header">
        <h1>3-Man Guildford</h1>
        <p className="desc">
          3 competitors split the events and solve them simultaneously; the team time is the
          slowest member&apos;s total. Shown globally, then the best team for each continent and
          country. Competitors only need a valid average in the events they&apos;re assigned. When
          a team is just as fast with one member doing nothing, that member shows &ldquo;no
          events&rdquo;.
        </p>
      </div>
      <TeamTable size={3} />
    </>
  );
}
