<?php

declare(strict_types=1);

// Run from a disposable Laravel project after adding predis/predis and
// league/flysystem-aws-s3-v3 and applying the values shown by `werd env`.
require getcwd() . '/vendor/autoload.php';
$app = require getcwd() . '/bootstrap/app.php';
$app->make(Illuminate\Contracts\Console\Kernel::class)->bootstrap();

$distance = Illuminate\Support\Facades\DB::selectOne("SELECT '[1,2,3]'::vector <-> '[4,5,6]'::vector AS distance");
if (abs((float) $distance->distance - sqrt(27)) > 0.0001) {
    throw new RuntimeException('Query pgvector errata');
}

Illuminate\Support\Facades\Cache::put('werd-smoke', 'redis-ok', 60);
if (Illuminate\Support\Facades\Cache::get('werd-smoke') !== 'redis-ok') {
    throw new RuntimeException('Cache Redis errata');
}

$subject = 'Werd smoke ' . bin2hex(random_bytes(4));
Illuminate\Support\Facades\Mail::raw('Mailpit integration test', function ($message) use ($subject) {
    $message->to('smoke@example.test')->subject($subject);
});

$s3 = new Aws\S3\S3Client([
    'version' => 'latest',
    'region' => getenv('AWS_DEFAULT_REGION') ?: config('filesystems.disks.s3.region'),
    'endpoint' => config('filesystems.disks.s3.endpoint'),
    'use_path_style_endpoint' => true,
    'credentials' => [
        'key' => config('filesystems.disks.s3.key'),
        'secret' => config('filesystems.disks.s3.secret'),
    ],
]);
$bucket = config('filesystems.disks.s3.bucket');
$persisted = false;
if ($s3->doesBucketExistV2($bucket)) {
    $persisted = Illuminate\Support\Facades\Storage::disk('s3')->get('smoke/test.txt') === 'rustfs-ok';
}
if (!$s3->doesBucketExistV2($bucket)) {
    $s3->createBucket(['Bucket' => $bucket]);
}
$path = 'smoke/test.txt';
Illuminate\Support\Facades\Storage::disk('s3')->put($path, 'rustfs-ok');
if (Illuminate\Support\Facades\Storage::disk('s3')->get($path) !== 'rustfs-ok') {
    throw new RuntimeException('Storage S3 errato');
}

echo "pgvector: OK\nRedis cache: OK\nMailpit SMTP: $subject\nRustFS upload/download: OK\n";
echo 'RustFS object persisted: ' . ($persisted ? 'OK' : 'first run') . "\n";
