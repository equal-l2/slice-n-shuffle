import init, { encode_image_buffer } from "./pkg/slice_n_shuffle.js";

await init();

/** @type {HTMLInputElement} */
const fileInput = document.getElementById("file_input");
/** @type {HTMLSelectElement} */
const xsplit_select = document.getElementById("xsplit");
/** @type {HTMLSelectElement} */
const ysplit_select = document.getElementById("ysplit");
/** @type {HTMLButtonElement} */
const run_button = document.getElementById("run");
/** @type {HTMLImageElement} */
const img = document.getElementById("img");

// https://stackoverflow.com/a/70544176
/**
 *
 * @param {HTMLImageElement} img
 * @param {URL} url
 * @returns {Promise<{width: number, height: number}>}
 */
function getImageDimensions(img, url) {
  return new Promise((resolve, reject) => {
    img.onload = () =>
      resolve({
        width: img.naturalWidth,
        height: img.naturalHeight,
      });
    img.onerror = (error) => reject(error);
    img.src = url;
  });
}

let imageWidth = null;
let imageHeight = null;

/**
 *
 * @param {HTMLSelectElement} select
 * @param {number} value
 */
function generateSplits(select, value) {
  const elems = [];
  for (let i = 0; i <= value; i++) {
    if (value % i == 0) {
      elems.push(new Option(i, i));
    }
  }
  select.replaceChildren(...elems);
}

/**
 *
 * @param {number} width
 * @param {number} height
 */
function setDimensions(width, height) {
  imageWidth = width;
  imageHeight = height;
  generateSplits(xsplit_select, imageWidth);
  generateSplits(ysplit_select, imageHeight);
  console.log(imageWidth, imageHeight);
}

async function openImage() {
  const inputFile = fileInput.files[0];
  if (inputFile === undefined) {
    throw new Error("No file is selected");
  }

  const allowedTypes = ["image/png", "image/jpeg"];
  if (!allowedTypes.includes(inputFile.type)) {
    throw new Error("The selected file is not a valid PNG or JPEG");
  }

  const imageUrl = URL.createObjectURL(inputFile);
  const res = await getImageDimensions(img, imageUrl);
  setDimensions(res.width, res.height);
}

async function run() {
  // Validate input file
  const inputFile = fileInput.files[0];
  if (inputFile === undefined) {
    throw new Error("No file is selected");
  }
  // Validate split
  const xsplit = xsplit_select.value;
  const ysplit = ysplit_select.value;
  if (xsplit === "" || ysplit === "") {
    throw new Error("Please specify xsplit and ysplit");
  }
  if (imageWidth % xsplit !== 0 || imageHeight % ysplit !== 0) {
    // Error Pearls by TabNine
    throw new Error("xsplit and ysplit must divide the image width and height");
  }

  const input_buf = await fileInput.files[0].arrayBuffer();
  const arr = new Uint8Array(input_buf);
  const ret = encode_image_buffer(arr, xsplit, ysplit);

  // TODO: use raw data to avoid encoding and decoding
  img.src = URL.createObjectURL(new Blob([ret], { type: "image/png" }));
}

fileInput.onchange = async () => {
  try {
    await openImage();
  } catch (e) {
    alert(e);
    fileInput.value = null;
  }
};
run_button.onclick = async () => {
  try {
    run_button.disabled = true;
    await run();
  } catch (e) {
    alert(e);
  } finally {
    run_button.disabled = false;
  }
};
