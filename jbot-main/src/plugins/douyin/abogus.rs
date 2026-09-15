use form_urlencoded::Serializer;
use jiff::Timestamp;
use rand::random_range;
use sm3::{Digest, Sm3};

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0";

const UIFID: &str = "4b5df8f2f1e37245005fd22980d950d15862f006c9917e4d56af34274e9d091069d16eb83533042983a5affc457fe8f67a16d2fcfc0989a420ac1cddee10b3d302489db181a6aba66248d26970a0dcbb8b9a1d4c610eed0a10c334f5dc1c9df4042146f15c0b93f43e06f172128eb12c5b9428e9462de86b3bd4ef8bdf22baeab16471327145507e7abc28f2a0b277845df7be865f67092c4187af60e4635c06";
const UIFID_SALT: &str = "A96D855A08C0A9707F8BEF0D9A527E4E";

// const ua_key: [u8; 3] = [0, 1, 14];
// S = list(range(256))
// j = 0
// for i in range(256):
//     j = (j + S[i] + ua_key[i % len(ua_key)]) % 256
//     S[i], S[j] = S[j], S[i]
const UA_KEY: [u8; 256] = [
    0, 218, 17, 20, 25, 23, 95, 116, 236, 14, 146, 5, 3, 151, 128, 186, 32, 114, 244, 80, 4, 46,
    36, 85, 213, 108, 174, 201, 63, 129, 47, 99, 38, 81, 150, 242, 69, 60, 72, 55, 192, 52, 10, 77,
    96, 141, 59, 62, 165, 204, 67, 120, 90, 240, 200, 94, 164, 221, 229, 98, 37, 145, 57, 230, 8,
    232, 169, 212, 132, 115, 209, 54, 110, 170, 39, 91, 167, 225, 207, 31, 210, 182, 152, 83, 144,
    195, 211, 161, 65, 29, 147, 183, 42, 97, 153, 50, 223, 43, 188, 79, 158, 187, 166, 179, 68,
    121, 44, 155, 75, 173, 252, 249, 11, 159, 27, 133, 58, 124, 243, 198, 239, 45, 241, 217, 1, 74,
    162, 103, 136, 226, 112, 199, 191, 21, 180, 163, 196, 157, 71, 56, 143, 234, 33, 205, 233, 34,
    181, 139, 119, 64, 193, 102, 76, 61, 15, 109, 160, 222, 111, 247, 202, 104, 70, 84, 178, 171,
    86, 140, 53, 238, 88, 255, 228, 175, 22, 118, 177, 197, 105, 82, 7, 154, 92, 190, 248, 246,
    214, 203, 135, 126, 123, 78, 18, 30, 35, 245, 12, 168, 51, 100, 227, 251, 235, 93, 49, 122,
    208, 206, 219, 142, 101, 176, 215, 130, 66, 117, 40, 134, 2, 253, 216, 189, 156, 125, 24, 16,
    26, 41, 220, 137, 106, 250, 172, 138, 237, 127, 19, 107, 148, 194, 89, 48, 254, 113, 231, 185,
    28, 224, 87, 73, 184, 9, 6, 13, 131, 149,
];

const SORT_INDEX: [u8; 44] = [
    18, 20, 52, 26, 30, 34, 58, 38, 40, 53, 42, 21, 27, 54, 55, 31, 35, 57, 39, 41, 43, 22, 28, 32,
    60, 36, 23, 29, 33, 37, 44, 45, 59, 46, 47, 48, 49, 50, 24, 25, 65, 66, 70, 71,
];
const SORT_INDEX_2: [u8; 44] = [
    18, 20, 26, 30, 34, 38, 40, 42, 21, 27, 31, 35, 39, 41, 43, 22, 28, 32, 36, 23, 29, 33, 37, 44,
    45, 46, 47, 48, 49, 50, 24, 25, 52, 53, 54, 55, 57, 58, 59, 60, 65, 66, 70, 71,
];

pub fn websign(query: &mut Vec<(&'_ str, String)>) {
    let now = Timestamp::now();
    let timestamp = now.as_second();
    query.push(("timestamp", timestamp.to_string()));
    query.push(("uifid", UIFID.to_string()));
    let params: String = Serializer::new(String::new())
        .extend_pairs(query.clone())
        .finish();
    let signature = format!("{UIFID}_{timestamp}_{UIFID_SALT}_{params}");
    let signature = format!("{:x}", md5::compute(signature));
    query.push(("x-secsdk-web-signature", signature));
}

pub fn abogus(
    query: &mut Vec<(&'_ str, String)>,
    body: &str,
    user_agent: Option<&str>,
    fp: Option<String>,
) {
    let user_agent = user_agent.unwrap_or(USER_AGENT);
    let browser_fp = fp.unwrap_or_else(fingerprint);

    let params: String = Serializer::new(String::new())
        .extend_pairs(query.clone())
        .finish();
    let start_encryption = Timestamp::now().as_millisecond();

    let array1 = sm3(&sm3_with_salt(&params));
    let array2 = sm3(&sm3_with_salt(body));
    let array3 = sm3(base64_encode(&rc4(user_agent)).as_bytes());

    let end_encryption = Timestamp::now().as_millisecond();

    let mut ab_dir = vec![0u8; 72];
    ab_dir[8] = 3;
    ab_dir[18] = 44;
    ab_dir[66] = 0;
    ab_dir[69] = 0;
    ab_dir[70] = 0;
    ab_dir[71] = 0;

    // 插入加密开始时间
    let start = start_encryption.to_be_bytes();
    ab_dir[20] = start[4];
    ab_dir[21] = start[5];
    ab_dir[22] = start[6];
    ab_dir[23] = start[7];
    ab_dir[24] = start[3];
    ab_dir[25] = start[2];

    // 插入请求头配置
    ab_dir[26] = 0;
    ab_dir[27] = 0;
    ab_dir[28] = 0;
    ab_dir[29] = 0;

    // 插入请求方法
    ab_dir[30] = 0;
    ab_dir[31] = 1;
    ab_dir[32] = 0;
    ab_dir[33] = 0;

    // 插入请求头加密 POST: 14 GET: 8
    ab_dir[34] = 0;
    ab_dir[35] = 0;
    ab_dir[36] = 0;
    ab_dir[37] = 8;

    // 插入请求体加密
    ab_dir[38] = array1[21];
    ab_dir[39] = array1[22];
    // 插入body加密
    ab_dir[40] = array2[21];
    ab_dir[41] = array2[22];
    // 插入ua加密
    ab_dir[42] = array3[23];
    ab_dir[43] = array3[24];

    // 插入加密结束时间
    let end = end_encryption.to_be_bytes();
    ab_dir[44] = end[4];
    ab_dir[45] = end[5];
    ab_dir[46] = end[6];
    ab_dir[47] = end[7];
    ab_dir[48] = ab_dir[8];
    ab_dir[49] = end[3];
    ab_dir[50] = end[2];

    ab_dir[51] = 0;
    ab_dir[52] = 0;
    ab_dir[53] = 0;
    ab_dir[54] = 0;
    ab_dir[55] = 0;
    ab_dir[57] = 239;
    ab_dir[58] = 24;
    ab_dir[59] = 0;
    ab_dir[60] = 0;

    // 插入浏览器指纹
    ab_dir[65] = browser_fp.len() as u8;

    let mut sorted_values: Vec<u8> = SORT_INDEX
        .iter()
        .map(|i| ab_dir.get(*i as usize).copied().unwrap_or(0))
        .collect();
    let mut ab_xor = ab_dir.get(SORT_INDEX_2[0] as usize).copied().unwrap_or(0);
    for index in 2..SORT_INDEX_2.len() {
        ab_xor ^= ab_dir.get(index).copied().unwrap_or(0);
    }

    sorted_values.extend_from_slice(browser_fp.as_bytes());
    sorted_values.push(ab_xor);

    let mut abogus_bytes = Vec::with_capacity(12 + sorted_values.len());
    random_bytes(&mut abogus_bytes);
    transform_bytes(&sorted_values, &mut abogus_bytes);

    let abogus = abogus_encode(&abogus_bytes);
    query.push(("a_bogus", abogus));
}

fn fingerprint() -> String {
    let inner_width = random_range(1024..=1920);
    let inner_height = random_range(768..=1080);
    let outer_width = inner_width + random_range(24..=32);
    let outer_height = inner_height + random_range(75..=90);
    let screen_x = 0;
    let screen_y = 0;
    let size_width = random_range(1024..=1920);
    let size_height = random_range(768..=1080);
    let avail_width = random_range(1280..=1920);
    let avail_height = random_range(800..=1080);

    format!(
        "{inner_width}|{inner_height}|{outer_width}|{outer_height}|\
      {screen_x}|{screen_y}|0|0|{size_width}|{size_height}|\
      {avail_width}|{avail_height}|{inner_width}|{inner_height}|24|24|Win32"
    )
}

const SM3_SALT: &[u8; 3] = b"cus";
const CHARS1: [char; 64] = [
    'D', 'k', 'd', 'p', 'g', 'h', '2', 'Z', 'm', 's', 'Q', 'B', '8', '0', '/', 'M', 'f', 'v', 'V',
    '3', '6', 'X', 'I', '1', 'R', '4', '5', '-', 'W', 'U', 'A', 'l', 'E', 'i', 'x', 'N', 'L', 'w',
    'o', 'q', 'Y', 'T', 'O', 'P', 'u', 'z', 'K', 'F', 'j', 'J', 'n', 'r', 'y', '7', '9', 'H', 'b',
    'G', 'c', 'a', 'S', 't', 'C', 'e',
];
const CHARS2: [char; 64] = [
    'c', 'k', 'd', 'p', '1', 'h', '4', 'Z', 'K', 's', 'U', 'B', '8', '0', '/', 'M', 'f', 'v', 'w',
    '3', '6', 'X', 'I', 'g', 'R', '2', '5', '+', 'W', 'Q', 'A', 'l', 'E', 'i', '7', 'N', 'L', 'b',
    'o', 'q', 'Y', 'T', 'O', 'P', 'u', 'z', 'm', 'F', 'j', 'J', 'n', 'r', 'y', 'x', '9', 'H', 'V',
    'G', 'D', 'a', 'S', 't', 'C', 'e',
];

fn sm3(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sm3::new();
    hasher.update(data);
    hasher.finalize().into()
}

fn sm3_with_salt(s: &str) -> [u8; 32] {
    let bytes = s.as_bytes();
    let mut data = Vec::with_capacity(bytes.len() + 3);
    data.extend_from_slice(bytes);
    data.extend_from_slice(SM3_SALT);
    sm3(&data)
}

fn rc4(s: &str) -> Vec<u8> {
    let mut a = UA_KEY.clone();
    let mut i: u8 = 0;
    let mut j: u8 = 0;
    let bytes = s.as_bytes();
    let mut res = Vec::with_capacity(bytes.len());
    for byte in bytes {
        i = ((i as usize + 1) % 256) as u8;
        j = ((j as usize + a[i as usize] as usize) as usize % 256) as u8;
        a.swap(i as usize, j as usize);
        let k = a[(a[i as usize] as usize + a[j as usize] as usize) % 256];
        res.push(byte ^ k)
    }
    res
}

fn base64_encode(input: &[u8]) -> String {
    let alphabet = &CHARS2;
    let mut result = String::with_capacity((input.len() + 2) / 3 * 4);
    let mut i = 0;

    while i + 2 < input.len() {
        let b1 = input[i] as usize;
        let b2 = input[i + 1] as usize;
        let b3 = input[i + 2] as usize;

        let i1 = b1 >> 2;
        let i2 = ((b1 & 0b11) << 4) | (b2 >> 4);
        let i3 = ((b2 & 0b1111) << 2) | (b3 >> 6);
        let i4 = b3 & 0b111111;

        result.push(alphabet[i1]);
        result.push(alphabet[i2]);
        result.push(alphabet[i3]);
        result.push(alphabet[i4]);

        i += 3;
    }

    let remain = input.len() - i;
    if remain == 1 {
        let b1 = input[i] as usize;
        let i1 = b1 >> 2;
        let i2 = (b1 & 0b11) << 4;
        result.push(alphabet[i1]);
        result.push(alphabet[i2]);
        result.push('=');
        result.push('=');
    } else if remain == 2 {
        let b1 = input[i] as usize;
        let b2 = input[i + 1] as usize;
        let i1 = b1 >> 2;
        let i2 = ((b1 & 0b11) << 4) | (b2 >> 4);
        let i3 = (b2 & 0b1111) << 2;
        result.push(alphabet[i1]);
        result.push(alphabet[i2]);
        result.push(alphabet[i3]);
        result.push('=');
    }

    result
}

fn random_bytes(q: &mut Vec<u8>) {
    for _ in 0..3 {
        let rd: u32 = random_range(0..10000);
        q.push((((rd & 255) & 170) | 1) as u8);
        q.push((((rd & 255) & 85) | 2) as u8);
        q.push((((rd >> 8) & 170) | 5) as u8);
        q.push((((rd >> 8) & 85) | 40) as u8);
    }
}

const BIG_ARRAY: [u8; 256] = [
    121, 243, 55, 234, 103, 36, 47, 228, 30, 231, 106, 6, 115, 95, 78, 101, 250, 207, 198, 50, 139,
    227, 220, 105, 97, 143, 34, 28, 194, 215, 18, 100, 159, 160, 43, 8, 169, 217, 180, 120, 247,
    45, 90, 11, 27, 197, 46, 3, 84, 72, 5, 68, 62, 56, 221, 75, 144, 79, 73, 161, 178, 81, 64, 187,
    134, 117, 186, 118, 16, 241, 130, 71, 89, 147, 122, 129, 65, 40, 88, 150, 110, 219, 199, 255,
    181, 254, 48, 4, 195, 248, 208, 32, 116, 167, 69, 201, 17, 124, 125, 104, 96, 83, 80, 127, 236,
    108, 154, 126, 204, 15, 20, 135, 112, 158, 13, 1, 188, 164, 210, 237, 222, 98, 212, 77, 253,
    42, 170, 202, 26, 22, 29, 182, 251, 10, 173, 152, 58, 138, 54, 141, 185, 33, 157, 31, 252, 132,
    233, 235, 102, 196, 191, 223, 240, 148, 39, 123, 92, 82, 128, 109, 57, 24, 38, 113, 209, 245,
    2, 119, 153, 229, 189, 214, 230, 174, 232, 63, 52, 205, 86, 140, 66, 175, 111, 171, 246, 133,
    238, 193, 99, 60, 74, 91, 225, 51, 76, 37, 145, 211, 166, 151, 213, 206, 0, 200, 244, 176, 218,
    44, 184, 172, 49, 216, 93, 168, 53, 21, 183, 41, 67, 85, 224, 155, 226, 242, 87, 177, 146, 70,
    190, 12, 162, 19, 137, 114, 25, 165, 163, 192, 23, 59, 9, 94, 179, 107, 35, 7, 142, 131, 239,
    203, 149, 136, 61, 249, 14, 156,
];

fn transform_bytes(s: &[u8], q: &mut Vec<u8>) {
    let mut a = BIG_ARRAY.clone();
    let mut index_b = a[1] as usize;
    let mut initial_value: usize = 0;
    for (index, char) in s.iter().enumerate() {
        let mut sum_initial = if index == 0 {
            initial_value = a[index_b] as usize;
            a[1] = initial_value as u8;
            a[index_b] = index_b as u8;
            index_b + initial_value
        } else {
            initial_value
        };
        sum_initial %= a.len();
        let value_f = a[sum_initial];
        let encrypted_char = char ^ value_f;
        q.push(encrypted_char);

        let value_e = a[(index as usize + 2) % a.len()];
        let sum_initial = (index_b as usize + value_e as usize) % a.len();
        initial_value = a[sum_initial] as usize;
        a[sum_initial] = a[(index as usize + 2) % a.len()];
        a[(index as usize + 2) % a.len()] = initial_value as u8;
        index_b = sum_initial;
    }
}

fn abogus_encode(input: &[u8]) -> String {
    let mut result = String::with_capacity((input.len() + 2) / 3 * 4);
    let mut i = 0;
    while i < input.len() {
        let n = if i + 2 < input.len() {
            ((input[i] as usize) << 16) | ((input[i + 1] as usize) << 8) | (input[i + 2] as usize)
        } else if i + 1 < input.len() {
            ((input[i] as usize) << 16) | ((input[i + 1] as usize) << 8)
        } else {
            (input[i] as usize) << 16
        };

        for (j, k) in [(18, 0xFC0000), (12, 0x03F000), (6, 0x0FC0), (0, 0x3F)] {
            if j == 6 && i + 1 >= input.len() {
                break;
            }
            if j == 0 && i + 2 >= input.len() {
                break;
            }
            result.push(CHARS1[(n & k) as usize >> j as usize]);
        }

        i += 3;
    }
    let r = 4 - result.len() % 4;
    if r > 0 {
        result.extend(std::iter::repeat('=').take(r as usize));
    }
    result
}
