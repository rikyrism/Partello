// Popular official trailers and music videos per genre (May–Oct 2026),
// each id verified as embeddable via YouTube oEmbed on 2026-10-27.
// Thumbnails are served by YouTube (i.ytimg.com) and belong to their owners.
// Refresh this list periodically; with YOUTUBE_API_KEY rooms can also search live.

export interface GenreVideo {
  id: string
  title: string
  channel: string
  /** Full thumbnail URL on i.ytimg.com; built from the id when missing. */
  thumb?: string
  /** Seconds, when known. */
  duration?: number | null
}

export interface Genre {
  /** English name; translated with t(). */
  name: string
  videos: GenreVideo[]
}

export const thumb = (id: string, size: 'hq' | 'mq' | 'maxres' = 'hq') => `https://i.ytimg.com/vi/${id}/${size}default.jpg`

/** The largest picture of a video: the chart's own, or YouTube's by id. */
export const still = (v: GenreVideo, size: 'hq' | 'mq' | 'maxres' = 'maxres') => v.thumb ?? thumb(v.id, size)

/** "Title | Official Trailer | Studio" → "Title". */
export const shortTitle = (title: string) => title.split(/\s[|–—]\s/)[0]!.trim()

export const GENRES: Genre[] = [
  {
    "name": "Action",
    "videos": [
      {
        "id": "irVNGjRFZGk",
        "title": "Avengers: Doomsday | Official Trailer | In Theaters December 18",
        "channel": "Marvel Entertainment"
      },
      {
        "id": "U6sbm1OaJb8",
        "title": "Street Fighter | New Trailer (2026 Movie)",
        "channel": "Paramount Pictures"
      },
      {
        "id": "7UIBOsuUwc4",
        "title": "Lanterns | Official Trailer | HBO Max",
        "channel": "HBO Max"
      }
    ]
  },
  {
    "name": "Adventure",
    "videos": [
      {
        "id": "gDJ-ini7fF0",
        "title": "NARNIA: THE MAGICIAN’S NEPHEW | Official Trailer | Netflix",
        "channel": "Netflix"
      },
      {
        "id": "f_bKjZeJBBI",
        "title": "The Odyssey | Official New Trailer",
        "channel": "Universal Pictures"
      },
      {
        "id": "JFQcDFhNh4o",
        "title": "Heart of the Beast | Official Trailer (2026 Movie)",
        "channel": "Paramount Pictures"
      }
    ]
  },
  {
    "name": "Animation",
    "videos": [
      {
        "id": "dtr5JL1zkiM",
        "title": "Wildwood – Official Trailer",
        "channel": "LAIKA Studios"
      },
      {
        "id": "wtUdmUTNbO4",
        "title": "Not Alone | Official Teaser Trailer",
        "channel": "Illumination"
      },
      {
        "id": "f7mFVeWnVLw",
        "title": "Forgotten Island | Official New Trailer",
        "channel": "Universal Pictures"
      }
    ]
  },
  {
    "name": "Comedy",
    "videos": [
      {
        "id": "FmM2giDwLAE",
        "title": "Violent Night 2 | Official Trailer",
        "channel": "Universal Pictures"
      },
      {
        "id": "JtswCuEQsEQ",
        "title": "Newlyweds | Official Trailer | NBC",
        "channel": "NBC"
      },
      {
        "id": "Lyztl-xEeFM",
        "title": "The Man with the Bag - Official Teaser | Prime Video",
        "channel": "Prime Video"
      }
    ]
  },
  {
    "name": "Crime",
    "videos": [
      {
        "id": "gnm4HgIAVmU",
        "title": "The Thomas Crown Affair | Official Teaser Trailer",
        "channel": "Amazon MGM Studios"
      },
      {
        "id": "RjEZaUBbUvU",
        "title": "The Further Mis-Adventures of Cliff Booth | Official Trailer | Netflix",
        "channel": "Netflix"
      },
      {
        "id": "B3tR6qQjbgI",
        "title": "UNABOMBER | Official Trailer | Netflix",
        "channel": "Netflix"
      }
    ]
  },
  {
    "name": "Documentary",
    "videos": [
      {
        "id": "HvW_N5p1q44",
        "title": "You Can See Everything | Official Trailer HD | A24",
        "channel": "A24"
      },
      {
        "id": "h3mRsvFmWi0",
        "title": "How to Live on Earth | Feature Documentary",
        "channel": "Open Planet"
      },
      {
        "id": "drszHQJtR7o",
        "title": "Don’t Look Back In Anger | Official Documentary Teaser",
        "channel": "Disney"
      }
    ]
  },
  {
    "name": "Drama",
    "videos": [
      {
        "id": "MqRa6X31lUk",
        "title": "The Gilded Age Season 4 | Official Trailer | HBO Max",
        "channel": "HBO Max"
      },
      {
        "id": "oRsAMzfvVGk",
        "title": "Batwara 1947 | Official Trailer",
        "channel": "Aamir Khan Talkies"
      },
      {
        "id": "M9NWvGZViCg",
        "title": "East of Eden | Official Trailer | Netflix",
        "channel": "Netflix"
      }
    ]
  },
  {
    "name": "Fantasy",
    "videos": [
      {
        "id": "0JlMjgqduVw",
        "title": "House of the Dragon Season 3 | Official Final Trailer | HBO Max",
        "channel": "HBO Max"
      },
      {
        "id": "A7lyvgnDuDE",
        "title": "Ebenezer | Official Trailer (2026 Movie)",
        "channel": "Paramount Pictures"
      },
      {
        "id": "SJVmeJaS44s",
        "title": "Harry Potter and the Philosopher's Stone | Official Teaser Trailer | HBO Max",
        "channel": "Harry Potter"
      }
    ]
  },
  {
    "name": "Horror",
    "videos": [
      {
        "id": "bEpTgowZ1dI",
        "title": "Other Mommy | Official Trailer",
        "channel": "Universal Pictures"
      },
      {
        "id": "OGO4Mqvo3jI",
        "title": "Clayface | Official Trailer",
        "channel": "DC"
      },
      {
        "id": "mNd1gb19A-c",
        "title": "RESIDENT EVIL – Official Trailer (4K)",
        "channel": "Sony Pictures Entertainment"
      }
    ]
  },
  {
    "name": "Music",
    "videos": [
      {
        "id": "FyS5dAywkEo",
        "title": "LISA - SaWaDiKa (Official Music Video)",
        "channel": "LISAOfficialVEVO"
      },
      {
        "id": "is8UDe2PhKQ",
        "title": "KATSEYE 'Hootie Frutti' Official MV",
        "channel": "HYBE LABELS"
      },
      {
        "id": "82-jTNka3uc",
        "title": "Ariana Grande - hate that i made you love me (official music video)",
        "channel": "ArianaGrandeVevo"
      }
    ]
  },
  {
    "name": "Romance",
    "videos": [
      {
        "id": "xwdamnfS6IM",
        "title": "The Love Hypothesis - Official Trailer | Prime Video",
        "channel": "Prime Video"
      },
      {
        "id": "HzGx48E64Xg",
        "title": "Love of Your Life - Official Trailer | Prime Video",
        "channel": "Prime Video"
      },
      {
        "id": "F5SpqQ8dNa8",
        "title": "Every Year After - Official Trailer | Prime Video",
        "channel": "Prime Video"
      }
    ]
  },
  {
    "name": "Science Fiction",
    "videos": [
      {
        "id": "NdvqHc56lE0",
        "title": "Dune: Part Three | Official Trailer",
        "channel": "Warner Bros."
      },
      {
        "id": "3oB9AxspVow",
        "title": "The End of Oak Street | Official Trailer",
        "channel": "Warner Bros."
      },
      {
        "id": "icDuEHSxE-w",
        "title": "Disclosure Day | Final Trailer",
        "channel": "Universal Pictures"
      }
    ]
  },
  {
    "name": "Thriller",
    "videos": [
      {
        "id": "ZybkHGaoPvQ",
        "title": "WAR | Official Trailer | HBO Max",
        "channel": "HBO Max"
      },
      {
        "id": "fneuVQ7uO_w",
        "title": "Drishyam The Conclusion | Official Trailer",
        "channel": "Star Studios"
      },
      {
        "id": "AaEy1f2Dgys",
        "title": "Nocturne — Official Trailer | Apple TV",
        "channel": "Apple TV"
      }
    ]
  }
]

/** Featured on the landing page (verified via oEmbed on 2026-10-27). */
export const FEATURED = {
  spongebob: { id: 'XdPt8QWTypI', title: 'The SpongeBob Movie: Search for SquarePants | Official Trailer', channel: 'Paramount Pictures' },
  spongebobTeaser: { id: 'w7t2gyIwvDo', title: 'The SpongeBob Movie: Search for SquarePants | Teaser Trailer', channel: 'Paramount Pictures' },
  wildwood: { id: 'dtr5JL1zkiM', title: 'Wildwood – Official Trailer', channel: 'LAIKA Studios' },
  notAlone: { id: 'wtUdmUTNbO4', title: 'Not Alone | Official Teaser Trailer', channel: 'Illumination' },
  forgottenIsland: { id: 'f7mFVeWnVLw', title: 'Forgotten Island | Official New Trailer', channel: 'Universal Pictures' },
} satisfies Record<string, GenreVideo>
