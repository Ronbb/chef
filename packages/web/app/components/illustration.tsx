import { useState } from "react";
import type { PublicLesson } from "@brioche/contracts/PublicLesson";
export function illustration(lesson: PublicLesson, id: string | undefined) {
  const asset = lesson.media.find((asset) => asset.assetId === id);
  if (asset) return asset;
  // Development fixture only; production staging requires all registered descriptors.
  if (!lesson.media.length && id === "art-bakery-morning")
    return {
      url: "/assets/bakery.svg",
      width: 640,
      height: 470,
      altZh: "社区面包店的清晨",
      creditZh: "",
    };
  return null;
}
export function Illustration({
  asset,
  className = "",
}: {
  asset: NonNullable<ReturnType<typeof illustration>>;
  className?: string;
}) {
  return <Image key={asset.url} asset={asset} className={className} />;
}
function Image({
  asset,
  className,
}: {
  asset: NonNullable<ReturnType<typeof illustration>>;
  className: string;
}) {
  const [failed, setFailed] = useState(false);
  return (
    <figure className={"lesson-media " + className}>
      {failed ? (
        <div className="media-fallback" role="img" aria-label={asset.altZh}>
          {asset.altZh}
        </div>
      ) : (
        <img
          src={asset.url}
          width={asset.width}
          height={asset.height}
          alt={asset.altZh}
          ref={(image) => {
            if (image?.complete && image.naturalWidth === 0) setFailed(true);
          }}
          onError={() => setFailed(true)}
        />
      )}
      {asset.creditZh && (
        <figcaption className="media-credit">{asset.creditZh}</figcaption>
      )}
    </figure>
  );
}
